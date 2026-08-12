# PHASE-04 progress log

One line per task, in the order they were done. Task codes are from section 9 of
`docs/plan/phases/PHASE-04-CLOUD-AI-GATEWAY.md`.

| Task | Role | Files touched | Tests added | Notes |
|---|---|---|---|---|
| T1 policy + threat model | PM, SEC | `docs/adr/ADR-0009-cloud-ai-policy.md` | - | Privacy, budget, determinism, fallback, keys. Records the TLS waiver and its expiry condition. |
| T2 error taxonomy | SRC | `crates/aura-core/src/errors/cloud.rs`, `errors.toml`, 14 runbooks | `error_registry` (existing, now covers CLOUD) | `AURA-CLOUD-6001`..`6014`. Three are not `fallback`, and each says why. |
| T3 frozen contract | CTO, AGT | `crates/aura-cloud/src/contract/cloud.rs` | - | `CloudTask`, `CloudResult`, `Source`, `Tier`, `PromptSpec`, `Validate`. Copied from section 5 and doc-commented. |
| T4 key storage | SEC | `crates/aura-cloud/src/keys.rs` | `tests/keys.rs` (14) | DPAPI / Keychain / libsecret by command invocation. The secret is only ever on stdin. |
| T5 provider layer | MBE | `provider.rs`, `anthropic.rs`, `openai.rs`, `google.rs`, `compat.rs` | `tests/budget.rs` retry cases | Four vendors, one shape. Retry, backoff with deterministic jitter, circuit breaker, model aliasing. |
| T6 transports | MBE | `http.rs`, `cassette.rs` | `tests/http.rs` (13) | A real HTTP/1.1 client; a cassette replayer; an offline refusal. No TLS - see ADR-0009. |
| T7 schema + repair | AGT | `schema.rs`, `validate.rs`, `repair.rs`, `fallback.rs` | `tests/schema.rs` (16) | A JSON Schema subset that refuses keywords it does not implement, rather than ignoring them. |
| T8 payload + redaction | SEC | `payload.rs`, `redact.rs` | `tests/privacy.rs` (14) | Contact sheets, crops, EXIF allow-list, pre-upload face blur, key scrubbing. |
| T9 storage | SRC | `cache.rs`, `audit.rs`, `budget.rs`, `migrations/0004_cloud_audit.sql` | `tests/budget.rs` (14) | Migration 4. The phase's DDL copied, then extended; the extension is in ADR-0009. |
| T10 consent gate | SRC | `crates/aura-catalog/src/consent.rs` | covered by `tests/gateway.rs` | Phase 01 froze `ConsentGate` and said phase 04 would be its first caller. It is. |
| T11 gateway | AGT, CTO | `gateway.rs` | `tests/gateway.rs` (17) | The seven steps. Every path writes an audit row, including the ones that send nothing. |
| T12 agent loop | AGT | `agent/{mod,loop,tools,scratchpad,limits}.rs` | `tests/agent.rs` (12) | Step cap, deterministic tool order, scratchpad in the audit row, cancel within one step. |
| T13 reference task | AGT | `tasks.rs` | `tests/schema.rs`, `tests/gateway.rs` | `SegmentNaming`, with the section 7 prompt and schema verbatim. `Scored` makes invariant 2 a compile error. |
| T14 cassettes | QAL | `tests/cloud/cassettes/*.json` (12) | - | Happy path, repair, unrepairable, truncation, 401 with an echoed key, 429, captive portal, three other vendors. |
| T15 IPC surface | CTO, SFE | `docs/adr/ADR-0010-cloud-ipc-surface.md`, `contract/ipc.rs`, `ui/src/ipc/types.ts` | `ipc_contract` (existing) | Ten commands, seven DTOs, one event stream. |
| T16 app commands | SFE | `crates/aura-app/src/cloud_commands.rs`, `state.rs` | - | The key goes one way. No `get_ai_key` exists. |
| T17 settings panel | SFE, MFE | `ui/src/components/AiKeysPanel.tsx`, `client.ts`, `App.tsx` | `AiKeysPanel.test.tsx` (13) | Key entry, Check, caps, privacy switches, spend meter, audit viewer. |
| T18 CI lints | CTO, SEC | `scripts/check-banned.sh` | - | No sockets outside `aura-cloud`; no key written anywhere but the credential store. |
| T19 phase gate | QAL | `crates/aura-cli/src/phase04.rs`, `justfile`, `ci.yml` | - | `aura-cli verify --phase 04`. Sixteen checks, no network. |
| T20 budgets | PERF | `perf/budgets.toml`, `crates/aura-perf/{src/lib.rs,tests/cloud_budgets.rs}` | `cloud_budgets.rs` (5) | Added count and cost budget kinds. Budget assertions moved to release. |
| T21 docs | DOC | `docs/using-your-own-ai-key.md`, `crates/aura-cloud/README.md`, `CHANGELOG.md` | - | The guide, the privacy answer, the cost guide, the audit explainer. |

## Measurements taken during the phase

All from `cargo test --release --package aura-perf` and
`aura-cli verify --phase 04` on the development machine (Windows 11, no GPU).

| Thing | Measured | Budget |
|---|---|---|
| Gateway overhead per call | 0.08 ms | 15 ms |
| Contact sheet build, 12 tiles | 167 ms | 250 ms |
| Calls for a 3,000 image wedding | 75 | 75 |
| Cost for that wedding | USD 1.04 | USD 1.50 |
| Cache hit rate on a re-run | 100 % | 70 % |
| Wall clock cost of a total outage | 9 ms against a 135 s floor, 0 % | 3 % |
| Uploaded payload, 12 tiles | 320 KB | 4 MB |

## Decisions taken during the phase, and why

**The cache lookup happens after the payload build, not before.** The phase
document's flow has it the other way round. It cannot: the cache key contains the
images' content hashes, and there are no hashes until the images exist. Costs
about 3 ms on a cache hit; the alternative costs correctness.

**`Scored` is a trait bound rather than a field.** Invariant 2 says every AI
decision carries confidence and reasons. Making it a bound on `CloudTask::Output`
means a task that cannot explain itself does not compile.

**Call identifiers come from a process-wide counter.** They were per-gateway, and
the phase gate found the bug: two gateways over one catalog - which is what
re-opening a project produces - derived the same id for the same cache key, and
the cache row overwrote the billed row. The spend meter read zero for a wedding
that had been billed.

**The sheet ceiling is 1536 px, not 768 px.** 768 is the per-tile limit from
section 2.1. Twelve 768 px tiles is a seven megapixel sheet costing about USD 0.03
of input, which the per-call ceiling would refuse - so `SegmentNaming` would have
silently fallen back on every segment. The arithmetic is in `payload.rs`.

**Budget assertions run in release.** A budget is a claim about the binary a
photographer runs. The payload builder is a pixel loop and is roughly ten times
slower unoptimised; a debug budget would be either failing or meaningless.
