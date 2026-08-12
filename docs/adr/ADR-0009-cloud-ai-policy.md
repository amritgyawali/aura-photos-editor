# ADR-0009 - Cloud AI policy: privacy, budget, determinism and the fallback rule

- **Status:** accepted
- **Date:** 2026-08-13
- **Deciders:** CTO, SEC (Security & Privacy Engineer), PM, AGT (AI Agent & Prompt Engineer), MLL
- **Phase:** 04

Section 8 step 1 of `docs/plan/phases/PHASE-04-CLOUD-AI-GATEWAY.md` asks for "ADR-0004
covering privacy, budget, determinism and the 'cloud is never required' rule". ADR-0004
is already taken by the RAW decode backend, and ADR numbers in this repository are
sequential and never reused, so this is that ADR under the next free number. The phase
document's cross-reference to "ADR-0004" should be read as "the cloud AI policy ADR".

## Context

Phase 04 turns the photographer's own API key into a reasoning layer that phases 07,
10, 12, 13, 24, 27, 28, 29 and 30 will all call. Three properties have to be settled
before any of those phases exist, because each one is impossible to retrofit:

1. **Privacy.** A wedding gallery is the most private set of photographs most people
   ever commission. Once a payload has left the machine it cannot be recalled.
2. **Cost.** The key belongs to the user. A runaway loop spends their money, not ours.
3. **Determinism.** Invariant 4 says the same inputs produce the same recipe. A
   sampling language model is the least deterministic component in the product.

And one property has to be settled because everything else depends on it:

4. **Cloud is never required.** Invariant 6. A photographer on a hotel wifi, on a
   plane, or with an expired key must still finish the wedding.

## Decision

### 1. One door, and the door is a port

`aura-cloud` is the only crate permitted to open a socket. `scripts/check-banned.sh`
fails the build on `TcpStream`, `reqwest`, `ureq`, `hyper`, `curl` and friends anywhere
in `crates/` outside `crates/aura-cloud/`, in the same way it already fails on `ort::`
outside `crates/aura-infer/`.

Inside the crate, the network itself is behind a `Transport` port, exactly as the
runtime is behind `Backend` in `aura-infer`. `Transport` is deliberately **not** a
frozen contract: it must be free to change when a TLS stack lands.

### 2. What this build ships, and what it does not

Three transports ship:

| Transport | What it does | Where it is used |
|---|---|---|
| `CassetteTransport` | Replays recorded responses keyed by the request digest | Every test, CI, `verify --phase 04` |
| `OfflineTransport` | Refuses every request with `AURA-CLOUD-6007` | Offline studio mode, and the offline acceptance test |
| `HttpTransport` | A real HTTP/1.1 client written in this crate over `std::net::TcpStream` | OpenAI-compatible endpoints reachable without TLS |

`HttpTransport` speaks the whole protocol - request framing, `Content-Length` and
chunked response bodies, header folding, status handling, connect/read/write deadlines -
and is a real network client. What it does not speak is TLS, so it reaches
`http://` endpoints only: a local Ollama, LM Studio, llama.cpp server or a LiteLLM
proxy on the studio network. Those are genuine, common deployments and the phase's
`compat` provider exists for exactly them.

**Public HTTPS endpoints (`api.anthropic.com`, `api.openai.com`,
`generativelanguage.googleapis.com`) therefore cannot be reached by this build.** The
provider modules that speak to them - request shaping, response parsing, error mapping,
model aliasing, cost tables - are complete and fully tested against cassettes recorded
from those APIs' documented shapes; only the socket underneath is missing.

The reason is the same one that kept ONNX Runtime out of phase 03 (ADR-0007). Every
production Rust TLS stack available today reaches a C or assembly cryptography core -
`ring` and `aws-lc-rs` both do - and this workspace is `#![forbid(unsafe_code)]` in every
crate with no C toolchain in its build requirements. `deny.toml` already records the
intended answer ("use rustls; no system OpenSSL dependency in a desktop app"). Adding
`rustls` is a supply-chain decision with a licence review, a build-requirements change
and a threat-model update attached, and it is not a decision to make as a side effect
of shipping a gateway.

**Waiver.** The `https` capability is waived for phase 04 with an expiry condition: it
lands as a `TlsStream` implementation of the `Stream` port behind a non-default cargo
feature, in the phase that first needs a public provider in production - phase 07 at the
earliest. Nothing above the `Transport` port changes when it does. This waiver is
carried in section 8 of `docs/progress/PHASE-04-EXIT.md`.

### 3. Privacy: what may leave the machine

**Derivatives only, always.** The payload builder accepts decoded pixels and produces
JPEG derivatives at most 768 px on the long edge. It has no path to a RAW file: it
cannot open one, and `PixelSource` provenance travels with every buffer it is handed.
An automated test walks every byte of every built payload looking for RAW container
magic, and fails if it finds any.

**Consent gates every call, per class.** `ProjectConsent` from phase 01 has been waiting
for this phase - its doc comment says so. `cloud_metadata` gates EXIF summaries and
counts; `cloud_derived_imagery` gates thumbnails and crops. `cloud_full_imagery` is
never requested by any task in this phase and the gateway refuses it outright.

**Stripped by default.** GPS, sub-second timestamps, camera serial numbers, owner and
artist tags, and filenames never enter a payload. The EXIF summary is a fixed, allow-listed
set of fields: camera model, lens, focal length, aperture, shutter, ISO, flash, and a
time *offset within the segment* rather than an absolute timestamp.

**Optional face blur before upload.** When the project asks for it, supplied face
rectangles are box-blurred into the derivative before encoding, and the blur is applied
to the bytes that are hashed, so the cache cannot serve an unblurred payload's answer to
a blurred request.

**Region pinning.** A project may pin an endpoint host. The gateway refuses to send to
any other host and records the refusal.

**Offline studio mode** disables the crate globally. Every task returns its local
fallback and the UI says which features are reduced.

### 4. Budget: the user's money

- Cost is **estimated before the call** from the token and image counts, using a
  per-model price table, and compared against three ceilings in order: the task's own
  `max_cost_usd`, the project's remaining cap, and the month's remaining cap.
- Three tiers per provider (`reasoning`, `balanced`, `cheap`). A task declares a minimum
  tier; the governor picks the cheapest acceptable one, and **downgrades one tier when
  less than 30 % of the cap remains**.
- A hard stop is not a failure: the pipeline continues with local fallbacks, every
  downgraded decision is recorded as such, and the gallery is complete.
- Batching is a budget mechanism, not an optimisation. Contact sheets collapse up to
  twelve decisions into one call, which is how the ≤ 75 calls per 3,000-image wedding
  budget is met.

### 5. Determinism: how a sampling model is made reproducible

- Temperature 0, `top_p` 1, no streaming, a fixed system prompt per task version.
- Every prompt is rendered with **sorted JSON keys** and hashed; the hash is stored in
  the audit row. A prompt edit without a task version bump is caught by the cassette
  golden tests, which key on that hash.
- The cache key is `blake3(task | task_version | prompt_hash | sorted image content
  hashes | model)`. It deliberately does **not** use the `Hash` impl the frozen
  `CloudTask::Input` bound provides: `DefaultHasher` is not guaranteed stable across
  compiler releases, and a cache key that changes when the toolchain does would silently
  re-bill the user after every upgrade. The `Hash` bound stays in the frozen trait, and
  is used only for in-process de-duplication within one run.
- Float inputs are quantised to integers before they enter an input struct - scores are
  per-mille `u16` - so a 1e-9 difference in a local classifier cannot miss the cache.
  This also satisfies the frozen `Hash` bound, which `f32` cannot.
- In CI the cassette transport makes the whole thing bit-reproducible: same fixtures,
  same recorded bytes, same decisions.

### 6. Fallback, and the fusion rule

Every task carries `local_fallback`, and the dispatch order is fixed:

```
policy refusal  -> local fallback (source = local_fallback)
cache hit       -> cached value   (source = cache)
provider error  -> local fallback
schema failure  -> one repair retry -> local fallback
success         -> cloud value    (source = cloud)
```

`CloudResult` always carries `source`, `confidence` and the model, so phase 13's Explain
panel can say where any decision came from.

**MLL's fusion rule, binding on every later phase:** cloud reasoning may not override a
local decision whose confidence is ≥ 0.90 unless the cloud response cites contradicting
visual evidence in `reasons[]`. When it does override, the conflict is written to the
audit row. Cloud proposes; deterministic code decides.

**Unknown enum values** map to `unknown` and subtract 0.20 from the reported confidence,
rather than being rejected - a model that invents a scene name is still telling us it
was uncertain.

### 7. Keys never touch our storage

Keys live in the OS credential store: DPAPI on Windows, Keychain on macOS, libsecret on
Linux. Because every crate is `#![forbid(unsafe_code)]`, the integration is by
**command invocation, not FFI** - `powershell` with `ConvertFrom-SecureString`,
`/usr/bin/security`, `secret-tool`.

The secret is written to the child process's **stdin** and never appears in `argv`,
because `argv` is world-readable on every platform we ship on. `keys.rs` builds a
`KeyCommand { program, args, stdin }` value and a unit test asserts that no element of
`args` ever contains the secret, for every platform's command shape, including the
delete and read paths.

Logs are scrubbed by a key-shaped-string redactor before any cloud value is traced, and
a test greps the audit rows, the cache rows, the plan file and the tracing output of a
full gate run for key-shaped strings.

## Consequences

**Good.** Later phases get one door with one policy. The pipeline is provably complete
without a network. CI tests the entire gateway with no network access at all. The user's
money is protected by an estimate made before the call, not a bill discovered after it.

**Bad.** Public HTTPS providers are unreachable until the TLS waiver is discharged, so
the shipped end-to-end path today is cassette, offline, or a local OpenAI-compatible
endpoint. The cassette responses are hand-recorded from published API shapes rather than
captured from live traffic, so a provider that changes its wire format will be caught by
a real integration run rather than by CI.

**Accepted risk.** The command-invocation keychain spawns a process per key operation,
which is slower than FFI and depends on `secret-tool` being installed on Linux. Both are
acceptable: key operations happen when a human types a key, not in a loop, and the Linux
path degrades to an explicit `AURA-CLOUD-6012` that names the missing tool.

## Alternatives considered

**Link `reqwest` with `rustls`.** Rejected for this phase: it reaches a C/assembly crypto
core, changes the build requirements, and the licence and advisory review belongs in its
own decision rather than buried in a gateway PR.

**Store the key encrypted in the catalog.** Rejected. The catalog is backed up, copied
to other drives and attached to support bundles. A key in it is a key in all of those.

**Let each calling phase own its own prompts and HTTP.** Rejected - this is the exact
failure mode `InferService` was created to prevent in phase 03, one phase later.

**Allow non-zero temperature for "creative" tasks such as captions.** Rejected for now.
Determinism is worth more than variety, and a caption task that wants variety can seed
the variety itself, deterministically, from the photo's content hash.

## Related

- `docs/adr/ADR-0007-inference-runtime.md` - the same port-with-a-waiver pattern
- `docs/adr/ADR-0010-cloud-ipc-surface.md` - the commands this policy is exposed through
- `docs/plan/CLAUDE.md` section 9 - the cloud rules this ADR implements
- `crates/aura-core/src/contract/consent.rs` - the gate, frozen in phase 01
