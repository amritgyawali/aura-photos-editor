# PHASE-04 exit report - Cloud AI Gateway & Agentic Reasoning Runtime

- **Date:** 2026-08-13
- **Branch:** `feat/phase-04-cloud-ai-gateway`
- **Gate:** `cargo run --release -p aura-cli -- verify --phase 04` exits 0
- **Signed off by:** CTO, PM, SEC, AGT, PERF, QAL

## 1. What shipped

One door to every cloud model, and a promise that nothing behind it is ever
required. `aura-cloud` is the only crate in the product allowed to open a socket;
`scripts/check-banned.sh` enforces that the way it already enforces one runtime
for local models.

The single feature, as the phase card states it: paste one AI API key and the app
gains a governed reasoning layer with tool-calling, strict JSON contracts,
caching, budget caps, redaction and a full audit trail.

## 2. Acceptance criteria (section 13)

| # | Criterion | Evidence |
|---|---|---|
| 1 | A user pastes a key, sees it validated, sets a cap, and sees spend after a run | `AiKeysPanel.tsx` + `check_ai_key` + `cloud_spend`; `AiKeysPanel.test.tsx` (13 tests); gate line `keys: round trip clean` |
| 2 | Every cloud task has a schema, a version, a prompt hash, a cost ceiling and a working local fallback | `CloudTask` makes all five compulsory - `local_fallback` is a required method. `tests/schema.rs` proves the schema is inside the supported subset; `tests/gateway.rs` proves the fallback answers on every failure path |
| 3 | With the network unplugged, a full wedding completes with decisions marked `local_fallback` | Gate line `offline: 75 segments - a 3,000 image wedding at one call per 40 - completed locally, every decision with reasons`; `tests/gateway.rs::with_the_network_unplugged_everything_still_completes` |
| 4 | The audit viewer can trace any AI decision to its call, model, tokens, cost and evidence | `cloud_calls` + the audit list in the panel. Every path writes a row, including refusals and fallbacks; gate line `audit: 6 rows for this job, $0.0309 billed, 2 answered locally` |
| 5 | No RAW or full-resolution pixels ever leave the machine; verified by an automated payload test | `tests/privacy.rs` - five refusal tests plus a byte scan of the built payload for five RAW container magics; gate line `payload: 12 tiles, 1536x864, 320 KB` with the same scan |
| 6 | Offline studio mode makes the crate inert and the UI honest about what is disabled | `tests/gateway.rs::offline_studio_mode_makes_the_crate_inert` asserts nothing is even built; `disabledReasons()` lists every reason, tested in `AiKeysPanel.test.tsx` |
| 7 | CI contains no network access and still fully tests the gateway via cassettes | 12 cassettes in `tests/cloud/cassettes/`. The whole 100-test Rust suite and the phase gate run against them; `every_cassette_is_played_by_something` fails if a recording goes stale |

**All seven pass.**

## 3. Test evidence

| Suite | Count | What it covers |
|---|---|---|
| `aura-cloud::gateway` | 17 | Happy path, cache, repair, unrepairable, truncation, offline, studio mode, project switch, consent, no key, cap, rate limit, captive portal, breaker, provider swap, cancel, cassette staleness |
| `aura-cloud::privacy` | 14 | RAW magic scan, tiled refusal, linear refusal, size limits, tile count, blur required, blur effectiveness, EXIF allow-list, key scrubbing, hash-not-a-key, audit scan, Debug scan, prompt guard |
| `aura-cloud::keys` | 14 | No secret in `argv` on three platforms, stdin only, macOS double-prompt, Debug renderings, fingerprint, absence handling, refusal handling, path escape, round trip, per-provider accounts |
| `aura-cloud::schema` | 16 | Subset parsing, unimplemented-keyword refusal, error ordering and completeness, nullable unions, pruning, enum coercion, fence and prose extraction, brace-in-string, truncation, vocabulary, invariant 2 |
| `aura-cloud::agent` | 12 | Write-tool refusal, duplicate names, stable ordering, finishing, cycle break, unknown tool, failing tool, step cap, scratchpad survival, cost ceiling, cancel, injected clock |
| `aura-cloud::http` | 13 | URL parsing, credential-in-URL refusal, request framing, chunked bodies, length-less bodies, status pass-through, HTTPS refusal, header injection, closed connection, garbage, digest, Debug |
| `aura-cloud::budget` | 14 | Pre-call pricing, wedding arithmetic, tier downgrade, cap refusal, ceiling refusal, soft cap, dual charging, region pin, tier fallback, cache key, prompt hash, cache round trip, backoff schedule |
| `aura-perf::cloud_budgets` | 5 | The five section 11 budgets |
| `ui` (vitest) | 13 new (33 total) | Provider labels, the spend sentence, every disabled reason, audit summaries, money |

Total: **105 new Rust tests, 13 new UI tests.** `cargo test --workspace` green;
`cargo clippy --workspace --all-targets -- -D warnings` clean;
`bash scripts/check-banned.sh` clean; `cargo xtask contracts --check` locked.

## 4. Performance (section 11)

| Metric | Budget | Measured | Verdict |
|---|---|---|---|
| Gateway overhead excluding provider latency | <= 15 ms per call | **0.08 ms** | pass |
| Cloud calls per 3,000-image wedding | <= 75 | **75** | pass |
| Cost per 3,000-image wedding | <= USD 1.50 | **USD 1.04** | pass |
| Cache hit rate on re-run | >= 70 % | **100 %** | pass |
| Failure impact on total pipeline time | <= 3 % | **0 %** (9 ms against a 135 s floor) | pass |
| Contact sheet build, 12 tiles | 250 ms (set from a run) | **167 ms** | pass |
| Uploaded payload, 12 tiles | 4 MB | **320 KB** | pass |

Measured on the development machine, Windows 11, no GPU, release build. Provider
latency is deliberately not budgeted: it is somebody else's service.

The cost figure is an estimate made by the cost governor itself against the
shipped price table, not a bill. `PRICE_TABLE_VERSION` is recorded on every audit
row so a real bill can be reconciled against the table that priced it.

## 5. Telemetry (section 11)

All four events are defined and emitted through `tracing`:

- `cloud.call` {task, task_version, model, tokens_in, tokens_out, cost_usd, latency_ms, status, retries}
- `cloud.fallback` {task, reason}
- `cloud.budget_stop` - via `AURA-CLOUD-6006` and `BudgetStore::note_stop`
- `cloud.cache` - via `ResponseCache::stats` and `FallbackLedger::cache_hit_rate`

`CloudEvent` is typed on both sides of the IPC boundary and not yet emitted to the
UI, for the same reason `IngestEvent` was not in phase 01 and `InferEvent` was not
in phase 03: the Tauri shell has not been launched on the development machine, so
an emitter would be code nobody has run.

## 6. Rollback

| Switch | Effect |
|---|---|
| Offline studio mode | The crate is inert. No key is read, no payload built, no socket opened. |
| Per-project cloud switch | Off for new projects by default. |
| `hard_stop = 0` on a budget row | The cap warns instead of stopping. |
| Migration 4 | Reversible in four statements, recorded in the migration file and asserted by the gate. Everything in those tables is an audit record or a cache; no photographic truth lives there. |
| `purge_cloud_cache(task, version)` | Forget a task version's answers after a prompt change. |

Removing the crate entirely leaves the product working: every caller of a
`CloudTask` gets the task's local fallback.

## 7. Known issues and deliberate omissions

**7.1 No TLS, so no public HTTPS provider (waived, ADR-0009).** `HttpTransport`
speaks the whole HTTP/1.1 protocol and reaches `http://` endpoints - a local
Ollama, LM Studio, llama.cpp or LiteLLM proxy, which is a real and common
deployment and is what the `compat` provider exists for. It cannot reach
`api.anthropic.com`, `api.openai.com` or `generativelanguage.googleapis.com`.

Those providers' request shaping, response parsing, error mapping, model aliasing
and pricing are complete and tested against cassettes recorded from their
documented shapes; the missing piece is the socket underneath. Adding a TLS stack
means reaching a C or assembly cryptography core in a workspace that is
`#![forbid(unsafe_code)]` throughout, which is a supply-chain decision with a
licence review and a threat-model update attached - not something to do as a side
effect of shipping a gateway.

*Expiry:* a `TlsStream` implementation of the `Stream` port behind a non-default
cargo feature, in the phase that first needs a public provider in production -
phase 07 at the earliest. Nothing above the `Transport` port changes when it does.

**7.2 Cassettes are hand-recorded, not captured.** They follow each vendor's
published response shape rather than live traffic, because no live traffic is
reachable from this build. A provider that changes its wire format will be caught
by the first real integration run rather than by CI. Mitigated by the fact that
all four providers' parsers are exercised on realistic bodies, including a 401
that echoes a key, a 429 with `Retry-After`, an HTML captive-portal page and a
response with no usage block at all.

**7.3 Price tables are defaults, not verified quotes.** The shipped per-million-
token figures are plausible published prices and are marked as overridable
defaults. `PRICE_TABLE_VERSION` is on every audit row so a bill can be
reconciled. **Before the first paying user, someone must check the three tables
against the vendors' published pages and bump the version.**

**7.4 Cloud confidence is not yet calibrated.** Section 6.2 says cloud confidence
is calibrated in phase 13 against outcomes. Until then a cloud `confidence` is the
model's own number, discounted by 0.20 per coerced enum value, and is *not* yet
comparable with a local model's. The fusion rule in `CloudResult::may_override`
is written so that it errs towards the local decision in the meantime.

**7.5 The agent loop has no production caller.** It is built, bounded and tested,
and phases 27 and 29 are its consumers. Shipping it now rather than then means
those phases inherit the step cap, the deterministic ordering and the scratchpad
rather than each inventing one.

**7.6 `SegmentNaming` has no local classifier behind it.** Its fallback uses the
scene guesses the caller supplies, because phase 07 builds the classifier. A
segment with no guesses gets `other` at confidence zero - honest, and the pipeline
still finishes.

## 8. Conditions carried forward

These follow the phase 02 conditions that ADR-0006 carried into phase 03, and are
carried again rather than quietly dropped.

| # | Condition | Owner | Trigger |
|---|---|---|---|
| C1 | Real camera files exercised through the RAW decoder | MLL | **Sev 2: the first real camera file reopens phase 02's criteria whatever phase is in flight** |
| C2 | A photographed ColorChecker measured end to end | COL | first real camera file |
| C3 | The three-OS CI matrix actually run | DEVOPS | first CI run on a machine with a Windows SDK |
| C4 | GPU throughput budgets (phase 03) | PERF | a GPU backend landing |
| C5 | TLS transport, so public HTTPS providers are reachable | MBE, SEC | phase 07, or the first user with a hosted key |
| C6 | Price tables checked against published vendor pages | PM | before the first paying user |
| C7 | Cassettes re-recorded from live traffic | QAL | with C5 |
| C8 | Cloud confidence calibrated against outcomes | MLL | phase 13 |
| C9 | Demo recording on a real 3,000-image wedding | PM | with C1 |

## 9. Definition of Done (section 14)

| Item | Status |
|---|---|
| Acceptance criteria verified by QA on the three reference weddings | **Partial.** All seven verified against synthetic fixtures and the cassette suite. The three reference weddings need C1. |
| Suites green on Windows (NVIDIA), Windows (DirectML) and macOS | **Carried (C3).** Green on the development machine; the matrix has never run. |
| Performance budget met, or a waiver recorded | **Met.** All five section 11 budgets pass with margin; the figures are in section 4. |
| Telemetry visible in the local dashboard and the aggregate pipeline | **Partial.** All four events emitted through `tracing`; the UI event stream is typed and not yet emitted (section 5). |
| Every AI decision surface returns `confidence` + `reasons[]` | **Met, and enforced by the compiler.** `Scored` is a bound on every task's `Output`. |
| Docs updated: module README, in-app help, CHANGELOG | **Met.** `crates/aura-cloud/README.md`, `docs/using-your-own-ai-key.md`, 14 runbooks, ADR-0009, ADR-0010, CHANGELOG. |
| Rollback path exists | **Met.** Section 6. |
| Demo recording on a real 3,000-image wedding | **Carried (C9).** |

## 10. What phase 05 may and may not assume

**May assume:**

- `CloudAiGateway::run` is the only way to reach a model provider, and it never
  fails for an ordinary cloud reason.
- Every answer carries `source`, `confidence` and `reasons[]`.
- A cap, an outage, a missing key and a malformed response are all the same kind
  of event to a caller: the local fallback answers.
- `cloud_calls` can be joined to any decision through `decision_ref`.

**May not assume:**

- That a public HTTPS provider is reachable (7.1).
- That a cloud `confidence` is comparable with a local model's (7.4).
- That the price of a call is exact rather than estimated (7.3).

**Must do, when adding a task:**

1. Bump `VERSION` on any prompt, schema or ceiling change.
2. Give it a local fallback that cannot fail for an ordinary reason.
3. Quantise every float in its `Input`.
4. Record a cassette for the happy path and at least one failure.
5. Keep it inside one call per 40 images, or argue for a change in an ADR.
