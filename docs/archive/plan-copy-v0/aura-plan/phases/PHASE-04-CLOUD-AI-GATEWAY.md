# Phase 04 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key)

> **Single feature shipped by this phase:** Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail.
>
> **Mission:** Turn the user's API key into an auditable, offline-tolerant reasoning capability that every later agent (Culling, QC, Album Story, Explain, Learning Loop) calls through one door - and that never becomes a hard dependency.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 04 of 30 |
| Epic | E1 - Foundation |
| Feature | Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail. |
| Depends on | Phases 01-03 |
| Unlocks | Phases 07, 10, 12, 13, 24, 27, 28, 29, 30 |
| Duration | 2 weeks |
| Primary owners | AI Agent & Prompt Engineer, Chief Architect / CTO Agent, Security & Privacy Engineer, Mid-Level Backend / Cloud Engineer |
| Risk level | High - cost, privacy and non-determinism all live here |
| Headline KPI | 100 % of responses schema-valid after one retry; cache hit rate >= 70 % on a re-run; cost per 3,000-image wedding <= USD 1.50 at default settings |
| Competitor being beaten | No competitor exposes a user-owned reasoning layer; this is a category-defining differentiator |

## 1. Why this phase exists

The user supplies an AI API key, so the product should extract maximum value from it - but a photo pipeline cannot be at the mercy of a rate limit or a flaky network. The gateway makes cloud reasoning a *bonus tier*: it upgrades decisions when available and degrades silently to local models when not.

Reasoning is only useful if it is structured. Free-text answers cannot drive a pipeline, so every call is a typed contract: JSON schema in, JSON schema out, validated, versioned, cached and logged. A malformed response is a handled error, never a corrupted gallery.

Weddings are private. The gateway is the single place where redaction, consent, per-project opt-in, region pinning and 'never upload originals' are enforced, so no later phase can accidentally leak a client's faces.

## 2. Scope contract

### 2.1 In scope

- `aura-cloud`: provider-agnostic client (Anthropic / OpenAI / Google / OpenAI-compatible endpoints) with model aliasing so prompts do not hard-code vendors.
- Secure key storage in the OS keychain (DPAPI / Keychain / libsecret), never in the catalog or logs; key validation and quota probe on entry.
- Typed task registry: every cloud task is a versioned `CloudTask` with input schema, output schema, prompt template, max tokens, temperature 0 default and a local fallback function.
- Strict JSON enforcement: schema validation, one repair retry with the validator error appended, then deterministic local fallback.
- Multimodal payload builder: downscaled proxy crops (default 768 px long edge), tiled context sheets (contact sheets of up to 12 thumbnails), EXIF summaries, never RAW files.
- Cost governor: per-project and per-month caps, live spend meter, per-task cost accounting, automatic downgrade to cheaper model tiers, hard stop with resumable state.
- Response cache keyed by (task, task_version, prompt_hash, image_content_hashes, model) with an on-disk SQLite store so re-running a wedding is nearly free.
- Audit trail: every call stored with prompt hash, tokens, latency, cost, model, decision id, so any AI decision in the product can be traced to its evidence.
- Privacy controls: project-level 'cloud AI off', face-blur-before-upload option, region/endpoint pinning, retention statement surfaced in the UI.
- Agent loop primitives: tool registry, bounded step count, deterministic tool ordering, structured scratchpad, timeout and cancellation.

### 2.2 Explicitly out of scope (do not build it here)

- Any specific reasoning feature (those live in Phases 07, 12, 13, 27, 29).
- Cloud GPU rendering or generative inpainting infrastructure (Phase 24 and 30).
- Fine-tuning or training against the user's key (never; training happens in `ml/`).
- Telemetry upload of image content (forbidden by the threat model).

## 3. Architecture and data flow

```text
caller (any later phase)
        |  CloudTask::<Name>{ typed input }
        v
  +-------------------- CloudAiGateway ---------------------+
  | 1 policy check (project opt-in, budget, privacy mode)   |
  | 2 cache lookup (task+version+content hashes+model)      |
  | 3 payload build (proxy crops <=768px, contact sheets)   |
  | 4 redaction (optional face blur, strip GPS/names)       |
  | 5 provider call (retry, backoff, timeout, cancel)       |
  | 6 JSON schema validate -> repair retry -> fallback      |
  | 7 cost accounting + audit row + cache write             |
  +------------------------+--------------------------------+
                           v
         typed output  OR  local fallback result (flagged source='local')
```

- Every result carries `source: 'cloud' | 'cache' | 'local_fallback'` plus `confidence`, so the QC agent and the Explain panel can always say where a decision came from.
- Temperature is 0 and prompts are hash-pinned, which makes cloud decisions reproducible enough for golden tests; the cache makes them fully reproducible in CI.
- The gateway is the only crate allowed to make outbound network calls; a CI lint enforces this.

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-cloud/src/{lib,gateway,provider,anthropic,openai,google,compat}.rs | Provider abstraction and HTTP clients. |
| `crates/aura-cloud/src/{tasks,schema,validate,repair,fallback}.rs` | Task registry, JSON schema validation and repair loop. |
| `crates/aura-cloud/src/{payload,redact,budget,cache,audit,keys}.rs` | Payload building, redaction, cost governor, cache, audit log, keychain. |
| `crates/aura-cloud/src/agent/{loop,tools,scratchpad,limits}.rs` | Bounded agent loop primitives reused by the QC and Album agents. |
| `crates/aura-catalog/migrations/0004_cloud_audit.sql` | `cloud_calls`, `cloud_cache`, `cloud_budget` tables. |
| `apps/desktop/src/routes/settings/AiKeys.tsx` | Key entry, provider choice, budget caps, privacy switches, live spend meter. |
| `docs/adr/ADR-0004-cloud-ai-policy.md` | Privacy, budget, determinism and fallback policy. |
| `tests/cloud/cassettes/*.json` | Recorded provider responses so CI never touches the network. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Cloud task contract (frozen)**

```rust
pub trait CloudTask: Send + Sync {
    const NAME: &'static str;
    const VERSION: u16;
    type Input: Serialize + Hash;
    type Output: DeserializeOwned + Validate;
    fn prompt(&self, input: &Self::Input) -> PromptSpec;      // system + user + images
    fn output_schema(&self) -> &'static str;                  // JSON Schema
    fn local_fallback(&self, input: &Self::Input) -> Result<Self::Output, AuraError>;
    fn max_cost_usd(&self) -> f32 { 0.02 }
}

pub struct CloudResult<T> {
    pub value: T,
    pub source: Source,          // Cloud | Cache | LocalFallback
    pub confidence: f32,
    pub model: String,
    pub tokens_in: u32, pub tokens_out: u32, pub cost_usd: f32,
    pub call_id: Uuid,
}
```

**Audit and cache tables**

```sql
CREATE TABLE cloud_calls (
  id TEXT PRIMARY KEY, project_id TEXT, task TEXT NOT NULL, task_version INTEGER NOT NULL,
  model TEXT NOT NULL, prompt_hash TEXT NOT NULL, image_hashes TEXT,
  tokens_in INTEGER, tokens_out INTEGER, cost_usd REAL,
  latency_ms INTEGER, status TEXT, retry_count INTEGER,
  decision_ref TEXT, created_at TEXT NOT NULL
);
CREATE TABLE cloud_cache (
  key TEXT PRIMARY KEY, task TEXT, response_json TEXT NOT NULL,
  created_at TEXT NOT NULL, hits INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE cloud_budget (
  project_id TEXT PRIMARY KEY, cap_usd REAL, spent_usd REAL NOT NULL DEFAULT 0,
  month TEXT, hard_stop INTEGER NOT NULL DEFAULT 1
);
```

## 6. Algorithm, model and implementation design

### 6.1 Where cloud reasoning is actually worth the money

- Scene naming and ritual identification for culturally specific weddings (Hindu, Nepali, Muslim, Christian) where a local classifier is uncertain - one contact-sheet call per timeline segment, not per image.
- Tie-breaking inside a burst when local scores are within noise (Phase 12): send 3-6 thumbnails and ask for a ranked choice with reasons.
- Natural-language explanations (Phase 13) generated from structured local evidence, so the reasoning is grounded, not invented.
- QC triage narrative and remediation planning (Phase 27) and album sequencing/captioning (Phase 29).
- Rule: never send more than ~1 call per 40 images by default; the pipeline must be able to run with `cloud=off` and lose no capability, only polish.

### 6.2 Determinism, validation and repair

- Temperature 0, top_p 1, fixed system prompt, sorted JSON keys, and a prompt hash committed in the audit row.
- Validate with a JSON Schema validator; on failure send exactly one repair message containing the validator error and the original response, then fall back locally.
- Any field the schema does not define is dropped; unknown enum values map to `unknown` and lower the confidence by 0.2.
- Cloud confidence is calibrated in Phase 13 against outcomes, so it is comparable to local model confidence.

### 6.3 Cost governor mechanics

- Estimate cost before calling from token/image counts; refuse or downgrade if the estimate exceeds the remaining budget.
- Three model tiers per provider (`reasoning`, `balanced`, `cheap`); tasks declare a minimum tier and the governor picks the cheapest acceptable one.
- Spend meter in the UI: 'this wedding has used $0.42 of your $5 cap'; on hard stop, the pipeline continues with local fallbacks and records which decisions were downgraded.
- Batching: contact sheets and multi-item questions collapse dozens of decisions into one call.

### 6.4 Privacy and security

- Keys live only in the OS keychain; logs redact anything key-shaped; a unit test greps artefacts for key patterns.
- Uploads are downscaled derivatives only - never the RAW, never the full-resolution export.
- Optional pre-upload face blur for extra-sensitive clients; GPS and client names stripped from payloads by default.
- Per-project switch plus a global 'offline studio mode' that disables the crate entirely; SEC signs off the payload builder.

## 7. Cloud AI usage (bring-your-own API key)

**Reference task implemented in this phase: `SegmentNaming` (proves the whole contract end to end)**

| Aspect | Specification |
|---|---|
| Model class | Vision-capable reasoning tier (e.g. Claude/GPT/Gemini class VLM), temperature 0 |
| Trigger | One call per timeline segment whose local scene confidence < 0.75 |
| Input sent | Contact sheet of up to 12 thumbnails (768 px each), segment start/end times, local top-3 scene guesses with scores, camera/flash summary |
| Cost control | Max 1 call per 40 images; cached by content hashes; downgrade to `balanced` tier when budget < 30 % remains |
| Offline fallback | Local scene classifier argmax from Phase 07 with `source='local_fallback'` |

System prompt contract:

```text
You are a wedding post-production analyst. You will receive a contact sheet of consecutive photographs from one wedding, a time range, and a local classifier's top guesses.
Task: name the wedding scene, name the specific ritual or activity if present, and state which cultural tradition the visual evidence supports.
Rules:
- Judge only from visible evidence. If evidence is weak, say so with low confidence rather than guessing.
- Use the controlled vocabulary supplied in `allowed_scenes` and `allowed_rituals`. If nothing fits, return "other" and describe it in `notes`.
- Never infer names, ethnicity or religion of individuals; describe the ceremony type only.
- Return ONLY JSON matching the provided schema. No prose, no markdown.
```

Required JSON response schema (validated; invalid = retry once, then fall back to local model):

```json
{
  "type": "object",
  "required": ["scene", "confidence", "reasons"],
  "properties": {
    "scene": { "type": "string" },
    "ritual": { "type": ["string", "null"] },
    "tradition": { "type": ["string", "null"] },
    "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
    "reasons": { "type": "array", "items": { "type": "string" }, "maxItems": 5 },
    "boundary_hint": { "type": ["string", "null"], "description": "index where the scene appears to change" },
    "notes": { "type": ["string", "null"] }
  },
  "additionalProperties": false
}
```

## 8. Implementation order (execute literally, in this order)

1. Write ADR-0004 covering privacy, budget, determinism and the 'cloud is never required' rule; SEC and CTO co-sign.
2. Implement keychain storage, key validation and the provider abstraction with one provider first.
3. Implement the `CloudTask` trait, schema validation, repair retry and local-fallback dispatch.
4. Implement the payload builder (crops, contact sheets, EXIF summary) with hard size limits and redaction.
5. Implement cache + audit tables and wire the cost governor with pre-call estimation.
6. Implement the bounded agent loop primitives (tools, scratchpad, step cap, timeout, cancel).
7. Ship `SegmentNaming` as the reference task with cassette-based tests.
8. Build the Settings > AI Keys UI: provider, key, caps, privacy switches, spend meter, audit viewer.
9. Add the CI lint that forbids network calls outside `aura-cloud` and the key-leak grep test.
10. Add the second and third provider behind the same aliases and verify identical schema behaviour.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Co-sign ADR-0004; enforce 'gateway is the only network door' in architecture lints | ADR + lint | 1 d |
| `AGT` | AI Agent & Prompt Engineer | Design the task registry, prompt-template system, repair loop, confidence mapping and the agent loop | `tasks`/`agent` modules | 6 d |
| `AGT` | AI Agent & Prompt Engineer | Implement `SegmentNaming` reference task with prompt versioning and cassettes | Reference task + tests | 2 d |
| `MBE` | Mid-Level Backend / Cloud Engineer | Provider clients, retry/backoff, streaming off, timeouts, rate-limit handling, model aliasing | `provider` layer | 5 d |
| `SEC` | Security & Privacy Engineer | Keychain integration, redaction rules, payload review, key-leak tests, threat-model update | Security sign-off | 4 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Cache + audit persistence, budget tables, migration 0004, cancellation plumbing | Storage layer | 3 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Settings > AI Keys, spend meter, audit viewer, privacy switches, offline-studio mode | UI shipped | 3 d |
| `MFE` | Mid-Level Frontend Engineer | Per-project cloud toggle, budget dialogs, downgrade notices, error toasts with plain language | UI polish | 2 d |
| `QAL` | QA Lead - Automation | Cassette harness, schema-violation fixtures, budget-exhaustion tests, offline tests, retry tests | CI gates | 4 d |
| `PM` | Product Manager Agent | Define which decisions may use cloud, default caps, and the user-facing privacy copy | Policy doc + copy | 2 d |
| `MLL` | ML Lead - Vision | Define how cloud confidence merges with local model confidence; guard against cloud overriding strong local evidence | Fusion rule spec | 1 d |
| `DOC` | Technical Writer | Write 'Using your own AI key', privacy FAQ, cost guide and the audit-trail explainer | Docs merged | 2 d |

### 9.1 Handoff chain for this phase

```text
PM policy + SEC threat model -> ADR-0004 (CTO)
            |
            v
  AGT (tasks, prompts, agent loop) <---> MBE (providers)
            |                                |
            v                                v
  SRC (cache/audit/budget)            SFE/MFE (settings, meter)
            \________ QAL (cassettes, offline, budget tests) ________/ -> CTO/PM gate
```

### How this agent team runs a phase (identical every time)

1. **Kickoff (PM + CTO + EM).** PM restates the feature as user stories, CTO writes/updates the ADR, EM cuts the task list from section 9 into the tracker.
2. **Design review (CTO + TLC + MLL + COL + UX).** Interfaces from section 5 are frozen before code. Any change after freeze needs an ADR amendment.
3. **Build in parallel lanes.** Core lane (TLC/SRC/SRG), ML lane (MLL/SRML/MLR/MLOPS), agent lane (AGT), UI lane (SFE/MFE/UX), data lane (DATA), platform lane (DEVOPS/SEC).
4. **Contract-first handoff.** A lane may only consume another lane's work through the frozen interface, using a stub/fixture until the real implementation lands.
5. **Code review chain.** Author -> peer in same lane -> lane lead -> CTO for anything touching an invariant. Two approvals minimum, one must be a lead.
6. **QA gate (QAL + QAIQ + PERF).** Unit + integration + golden-image + perceptual + performance suites must be green on the reference weddings.
7. **Phase gate (CTO + PM + EM).** All acceptance criteria in section 13 pass, telemetry is live, docs updated, demo recorded. Only then does the next phase start.
8. **Escalation.** Any blocker older than one working day goes to EM; any invariant conflict goes to CTO; any "we should ship it slightly broken" goes to PM and is written down.

### Branch, commit and PR rules

- Branch: `feat/phase-NN-<slug>`; one PR per task group, never one giant PR per phase.
- Conventional Commits (`feat(core): ...`, `fix(ml): ...`, `perf(render): ...`, `test(qa): ...`, `docs: ...`).
- Every PR states: what changed, which acceptance criterion it advances, benchmark delta, and screenshots or golden-image diffs when pixels change.
- CI must be green: `fmt`, `clippy -D warnings`, `cargo test`, `pytest`, `vitest`, golden-image diff, benchmark regression guard (<= 5 % slower), model-hash check.


## 10. Test plan

### 10.1 Phase-specific tests

- Offline: with the network disabled every cloud task returns a local fallback and the pipeline completes.
- Schema violation: malformed, truncated and extra-field responses trigger exactly one repair, then fallback; never a panic.
- Budget: a project at its cap stops calling, records downgrades, and still produces a complete gallery.
- Cache: re-running the same wedding produces >= 70 % cache hits and identical decisions.
- Privacy: payload inspector test asserts no RAW bytes, no GPS, no filenames with client names, and blur applied when enabled.
- Key safety: logs, crash dumps and telemetry artefacts contain no key-shaped strings.
- Provider swap: the same task on three providers yields schema-valid output and comparable decisions on the fixture set.

### 10.2 Standing test matrix (applies to every phase)

| Layer | What it proves |
|---|---|
| Unit | Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy. |
| Property/fuzz | Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects. |
| Golden image | Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed. |
| Perceptual (human) | QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required. |
| Performance | Throughput, wall clock, peak RAM, peak VRAM on the three reference machines. |
| Resume/kill | Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption. |
| Regression | Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress. |

Reference machines: RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

## 11. Performance budget and telemetry

| Metric | Budget |
|---|---|
| Gateway overhead excluding provider latency | <= 15 ms per call |
| Cloud calls per 3,000-image wedding (default) | <= 75 |
| Cost per 3,000-image wedding (default settings) | <= USD 1.50 |
| Cache hit rate on re-run | >= 70 % |
| Failure impact on total pipeline time | <= 3 % when all cloud calls fail |

Telemetry events (local-first, opt-in aggregation):

- `cloud.call` {task, task_version, model, tokens_in, tokens_out, cost_usd, latency_ms, status, retries}
- `cloud.fallback` {task, reason}
- `cloud.budget_stop` {project, cap_usd, spent_usd}
- `cloud.cache` {hit_rate, entries, bytes}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Provider outage or rate limits stall a wedding | Bounded retries, circuit breaker, immediate local fallback, and no pipeline stage may block on cloud results. |
| Runaway cost | Pre-call estimation, hard caps, batching, tier downgrade, and per-task cost ceilings. |
| Prompt drift changes decisions between releases | Prompt hashing, task versioning, cassette golden tests, and a changelog entry required for any prompt edit. |
| Privacy incident | Derivative-only uploads, optional face blur, region pinning, audit log, SEC sign-off on the payload builder, and offline-studio mode. |
| Cloud reasoning overrides better local evidence | Fusion rule: cloud may not override a local decision with confidence >= 0.9 unless it supplies contradicting visual evidence, and the conflict is logged. |

## 13. Acceptance criteria

- [ ] A user pastes a key, sees it validated, sets a budget cap, and can immediately see spend after running a wedding.
- [ ] Every cloud task has a schema, a version, a prompt hash, a cost ceiling and a working local fallback.
- [ ] With the network unplugged, a full wedding completes end to end with decisions marked `local_fallback`.
- [ ] The audit viewer can trace any AI decision to its call, model, tokens, cost and evidence.
- [ ] No RAW or full-resolution pixels ever leave the machine; verified by an automated payload test.
- [ ] Turning on 'offline studio mode' makes the crate inert and the UI honest about what is disabled.
- [ ] CI contains no network access and still fully tests the gateway via cassettes.

## 14. Definition of Done (phase gate)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

Inherited invariants that this phase must not break:

- **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
- **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
- **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
- **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
- **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
- **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
- **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
- **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
- **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

## 15. Claude Code execution prompt (copy-paste this)

```text
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 04 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key).

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-04-CLOUD-AI-GATEWAY.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Paste one AI API key and the app gains a governed reasoning layer: VLM/LLM calls with tool-calling, strict JSON contracts, caching, budget caps, redaction and a full audit trail.

Rules:
  - Do not start Phase 5. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-cloud/src/{lib,gateway,provider,anthropic,openai,google,compat}.rs, `crates/aura-cloud/src/{tasks,schema,validate,repair,fallback}.rs`, `crates/aura-cloud/src/{payload,redact,budget,cache,audit,keys}.rs`, `crates/aura-cloud/src/agent/{loop,tools,scratchpad,limits}.rs`, `crates/aura-catalog/migrations/0004_cloud_audit.sql`, `apps/desktop/src/routes/settings/AiKeys.tsx`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-04-cloud-ai-gateway and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-04.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-04-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-04-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-04-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 04 of 30 - Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key) - part of the AURA Wedding AI master build plan.*
