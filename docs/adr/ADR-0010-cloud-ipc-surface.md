# ADR-0010 - Extending the frozen IPC surface for cloud AI

- **Status:** accepted
- **Date:** 2026-08-13
- **Deciders:** CTO, SFE (Senior Frontend Engineer), MFE, SEC, PM
- **Phase:** 04

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen
contracts, digested in `contracts.lock`. Section 9 of the phase document assigns
SFE "Settings > AI Keys, spend meter, audit viewer, privacy switches,
offline-studio mode" and MFE "per-project cloud toggle, budget dialogs, downgrade
notices". Acceptance criteria 1, 4 and 6 all require a panel. None of it is
possible without new commands. Changing a frozen contract requires an ADR first
and a re-lock second; this is that ADR, following the pattern ADR-0005 set in
phase 02 and ADR-0008 in phase 03.

The phase document also names the UI file as
`apps/desktop/src/routes/settings/AiKeys.tsx`. This repository's UI lives in
`ui/src/components/`, as established in phase 01 and followed by
`HardwarePanel.tsx` in phase 03. The panel is therefore
`ui/src/components/AiKeysPanel.tsx`; the divergence is layout, not design.

## Decision

Ten commands, seven DTOs and one event stream are added. Nothing existing
changes, so every phase 01, 02 and 03 caller keeps working.

| Command | Returns | Why it exists |
|---|---|---|
| `cloud_status` | `CloudStatusDto` | Provider, endpoint, whether a key is stored, and what is switched off |
| `set_ai_key` | `CloudStatusDto` | The one command that carries a key, and it carries it one way |
| `clear_ai_key` | `CloudStatusDto` | Forget a key without leaving a copy |
| `check_ai_key` | `KeyCheckDto` | Prove the key works, in one round trip, before a wedding depends on it |
| `set_cloud_budget` | `CloudSpendDto` | The per-job and per-month caps |
| `set_cloud_privacy` | `CloudStatusDto` | Per-project switch, face blur, offline studio mode |
| `cloud_spend` | `CloudSpendDto` | The live spend meter |
| `cloud_calls` | `CloudCallDto[]` | The audit viewer |
| `cloud_cache_stats` | `CloudCacheStatsDto` | What the response cache is holding |
| `purge_cloud_cache` | `number` | Forget a task version's answers after a prompt change |

`CloudEvent` (`call`, `fallback`, `budgetStop`, `cache`) mirrors `PreviewEvent`
and `InferEvent`, and carries exactly section 11's four telemetry events.

### What the surface deliberately carries

**A fingerprint, not a key.** `CloudStatusDto.keyFingerprint` is four characters
from each end - `sk-a...9xQz` - and nothing in between. A photographer with three
keys can tell which one is stored; a screenshot in a support ticket reveals
nothing. Keys shorter than twelve characters get `****` rather than a
fingerprint that would be most of the secret.

**Every reason a call would not be made.** Offline studio mode, a project switch
that is off, a missing key, an open circuit breaker and a transport with no
network are all reported with their own sentence. A panel that shows a working
system which happens to make no calls generates support tickets; this one answers
them.

**Local decisions in the audit viewer.** `CloudCallDto.source` is `cloud`,
`cache` or `local_fallback`, and `fallbackReason` says why for the last of them.
The rows worth reading are usually the ones where nothing was sent.

**Money as a float, and the DTO is therefore not `Eq`.** Providers bill in
fractions of a cent. `CloudSpendDto` and `CloudCallDto` derive `PartialEq` only,
which is the same accommodation `CacheStatsDto` made in phase 02 for its hit rate.

### What it deliberately does not carry

- **No command returns a key.** There is no `get_ai_key` and there will not be
  one. The key crosses the boundary once, inwards, on its way to the operating
  system's credential store.
- **No file paths.** The credential blob's location, the catalog's path and the
  cassette directory are all absent. Article IX rule S4 keeps paths out of
  anything that can reach a log or a support bundle.
- **No prompt or response text.** The audit viewer shows the prompt *hash*, the
  token counts and the cost. A viewer that showed the prompt would show the
  contact sheet's provenance and, eventually, a client's details.
- **No `cloud_full_imagery` setter.** `SetCloudPrivacyInput` has switches for the
  project, for blur and for offline mode. It has none for full-resolution egress,
  because no task asks for it and the gateway refuses that data class
  unconditionally. A setter would be the first step towards something that did.

### The commands' cost

Every command reads state already in memory or one indexed row, except
`check_ai_key`, which is explicitly the command that spends a round trip in front
of the user. It uses a fifteen-second ceiling and **no retries**: a Check button
that took ninety seconds to fail three times would be worse than no button. It
also bypasses the circuit breaker, because resetting the breaker is one of the
things pressing Check is for.

## Consequences

**Good.** The panel can tell the whole truth: what is stored, what is off, what
has been spent, and what every decision cost. Phase 13's Explain panel reads the
same `CloudCallDto` rows, so tracing a decision to its evidence needs no new
surface.

**Bad.** Ten commands is a large single addition to a frozen contract, and
`contracts.lock` moves for both files at once. The alternative - adding them over
three phases as each consumer appeared - would have meant three ADRs and three
re-locks for the same surface.

**Accepted risk.** `CloudEvent` is typed on both sides and not yet emitted, for
the same reason `IngestEvent` was not in phase 01 and `InferEvent` was not in
phase 03: the Tauri shell has not been launched on the development machine, so an
emitter would be code nobody has run. The types are frozen now so that the phase
which does launch it has nothing to negotiate.

## Related

- `docs/adr/ADR-0009-cloud-ai-policy.md` - the policy these commands expose
- `docs/adr/ADR-0008-inference-ipc-surface.md` - the pattern this follows
- `crates/aura-app/src/cloud_commands.rs` - the implementations
- `ui/src/components/AiKeysPanel.tsx` - the panel
