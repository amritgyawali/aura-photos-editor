# AURA Engineering Constitution

**The rules, regulations and standing orders for building AURA as an enterprise-grade product.**

Written by the Chief Architect. Ratified by the whole agent team. Binding on every human and
every AI agent that touches this repository.

> This document is not advice. It is the operating law of this project. Where a phase plan, a
> ticket, a prompt or an opinion conflicts with this constitution, this constitution wins. If a rule
> here is wrong, change the rule through the amendment process in Article XXII - do not quietly
> ignore it.

---

## Preamble: what enterprise-grade actually means

After twenty-five years of building software, I can tell you that "enterprise-grade" has nothing to
do with how the product looks, how many features it has, or how impressive the AI sounds in a demo.
It is a set of properties that only become visible when things go wrong.

Enterprise-grade means:

1. **It behaves predictably.** The same input produces the same output, today and in eighteen
   months, on a different machine, after three model updates.
2. **It fails safely.** Every failure has a defined behaviour, a clear message, and no data loss.
   Nothing is ever silently dropped, corrupted or half-written.
3. **It can be operated by someone who did not build it.** Runbooks, logs, support bundles, error
   catalogues, rollback procedures.
4. **It can be changed safely.** Small reversible changes, comprehensive gates, tested rollback,
   feature flags on anything risky.
5. **It can be audited.** Every decision the system made is recorded with its reason, its inputs and
   the version of the component that made it.
6. **It respects the customer absolutely.** Their data, their originals, their manual work, their
   privacy, their clients, and their right to disagree with the machine.
7. **It is honest.** The documentation matches the behaviour, the marketing matches the
   documentation, and the confidence number matches the observed accuracy.

We are building software that will process the only photographs that exist of the most important day
in a stranger's life. There is no second shoot. There is no "we will fix it in the next release" for
an album that has already been delivered. That single fact justifies every constraint in this
document.

A note on ambition and discipline: this plan describes a system that is more ambitious than most
venture-funded teams attempt. The only way a small team with almost no budget delivers something
like this is by being **relentlessly disciplined about the invariants and relentlessly ruthless about
scope**. Discipline is what buys us the right to be ambitious.

---

## Article I: The Ten Laws

These are absolute. They are not trade-offs to be balanced against schedule. Every agent memorises
them; every pull request is judged against them.

1. **The photographer's originals are read-only, forever.** We never write to, move, rename or
   modify a RAW file. Every edit lives in a recipe. A bug that touches an original is a Severity 1
   incident.

2. **Manual work is sacred.** Once a human has edited a value, no automated pass may overwrite it.
   `user_edited_fields` is checked before every write, in every code path, without exception.

3. **Determinism is a feature.** Same input, same models, same version, same output - bit-identical
   where possible, within a published tolerance where hardware forbids it. Nondeterminism is a
   defect, not a characteristic.

4. **Offline is the default, not the fallback.** Every feature completes with the network cable
   unplugged. Cloud reasoning is an optional accelerant with a tested local fallback. This is proven
   by a network-disabled CI job on every commit.

5. **Never fail silently.** No `unwrap`, no `expect`, no empty catch block, no swallowed error, no
   default value substituted for a failure. Every error is typed, logged, surfaced and actionable.

6. **Every automated decision is explainable and reversible.** One sentence a photographer
   understands, one click to override. An AI that cannot be argued with will not be trusted, and an
   untrusted autonomous product has no market.

7. **No unbudgeted work.** Every stage has a latency, throughput and memory budget, measured on
   three named reference machines and asserted in CI. Regressions beyond five per cent fail the
   build.

8. **Every long operation is cancellable and resumable.** Cancellation within 250 milliseconds.
   `kill -9` at any moment leaves a catalogue that opens and resumes cleanly.

9. **Licence-clean or it does not ship.** Every dependency, every model weight, every training image
   has a documented commercial-safe licence. Research-only assets are physically quarantined.

10. **Privacy is structural.** Client images never leave the machine without explicit per-project
    consent. Face embeddings stay local. No file paths in logs, telemetry or crash reports - paths
    contain client names.

**Two corollaries that carry the same weight:**

- **Small, reviewable, reversible.** Pull requests under 400 changed lines, behind a feature flag if
  risky, revertible without a migration.
- **Correct, then beautiful, then fast - in that order, always.** Anyone who inverts this ordering is
  building a different product than the one we promised.

---

## Article II: Architecture rules

**A1. Boundaries are explicit and enforced by the compiler.** Every crate has one responsibility and
a documented public surface. Internals stay private. If two crates need to know each other's
internals, the boundary is drawn in the wrong place.

**A2. The dependency rule.** Dependencies point inward, toward the domain. `aura-core` depends on
nothing of ours. Infrastructure (RAW decoding, GPU, SQLite, network) depends on the domain, never
the reverse. The domain must be testable with no filesystem, no GPU and no network.

**A3. Ports and adapters for everything replaceable.** Inference runtime, RAW decoder, storage,
cloud provider, telemetry sink - each behind a trait with at least two implementations, one of which
is a test double. If it cannot be faked, it cannot be tested.

**A4. Contracts before code.** Types, traits, schemas and error enums are written, reviewed and
merged before implementation begins. Two agents may then work in parallel without collision. This is
the single highest-leverage rule in this document for a multi-agent team.

**A5. One writer per data domain.** Exactly one crate owns writes to each table. Everything else
reads or requests. Concurrent writers to the same domain is a corruption bug waiting for its moment.

**A6. No circular dependencies, ever.** Enforced by tooling in CI. A cycle means the layering is
wrong; fix the design, do not add a shim.

**A7. Architectural decisions are recorded.** Any choice that is expensive to reverse gets an ADR:
context, options considered, decision, consequences. Numbered, immutable, superseded rather than
edited.

**A8. Optional means optional.** Any component that can be absent - a model pack, a GPU, the
network, a cloud key - has a defined degraded behaviour that is tested with the component removed.

**A9. The UI is a thin presentation layer.** No business logic, no image processing, no heavy
computation in the renderer. All work crosses a typed IPC boundary into Rust.

**A10. Design for the 4,000-file wedding from the first line.** Never write code that works at a
hundred images and needs rewriting at four thousand. Stream, page, batch and bound everything.

---

## Article III: Code rules

### Universal

- **Readability outranks cleverness.** Code is read fifty times more often than it is written, and in
  this project much of it is read by AI agents that need unambiguous structure.
- **Functions do one thing.** Target 40 lines, hard limit 80. Files target 400 lines, hard limit 800.
  A long file is a missing module.
- **Names state intent and units.** `timeout_ms`, `max_vram_bytes`, `keeper_threshold`. Never `tmp`,
  `data`, `helper`, `manager2`.
- **Comments explain why, never what.** The code says what. A comment earns its place by recording a
  decision, a constraint, a surprise or a reference to an ADR.
- **No magic numbers.** Every threshold is a named constant in one place, with a comment stating
  where it came from and who owns it.
- **Delete dead code immediately.** Version control is the archive. Commented-out blocks are debt
  with no interest payments.

### Rust (the core)

- **No `unwrap`, `expect` or `panic!` in non-test code.** Enforced by lint. Errors are typed with
  `thiserror`, propagated with `?`, and contextualised at the boundary.
- **`unsafe` requires an ADR and two reviewers.** Every `unsafe` block carries a comment proving why
  it is sound. FFI lives in one quarantined module per foreign library.
- **`clippy::pedantic` clean.** Warnings are errors in CI. Allowances are per-line with a reason.
- **No blocking on an async runtime.** No blocking calls in async contexts, no `block_on` inside a
  worker. CPU work goes to a dedicated pool.
- **Bounded channels only.** Unbounded channels are memory leaks that wait for a large wedding.
- **`#[must_use]` on anything ignorable.** Silently dropping a result is how invariants die.

### TypeScript (the interface)

- **`strict` mode, and `any` is banned.** Use `unknown` and narrow. Types crossing IPC are generated
  from Rust, never hand-maintained.
- **No business logic in components.** Fetch, present, delegate.
- **Every async surface handles four states.** Loading, empty, error, cancelled. A screen without
  them is unfinished, not "mostly done".
- **No index-based list keys** where order can change; use stable identifiers.

### Python (training only)

- **Python never ships to a customer.** It trains and evaluates. Every artefact crossing into the
  product is ONNX plus a model card.
- **Every run is a config plus a seed.** No notebook is a source of truth. Reproducibility is
  recorded: config, seed, data manifest hash, commit.
- **Type hints and `ruff` clean** on anything that survives past one experiment.

---

## Article IV: Determinism and reproducibility

**D1. Same inputs, same outputs.** Given identical originals, recipes, model versions and engine
version, output pixels are identical on the same hardware and within a published tolerance across
providers. This is verified by the golden-image suite on every commit.

**D2. No unordered iteration in output paths.** Never let directory order or hash-map order influence
results. Sort explicitly.

**D3. Seeds are recorded, never implicit.** Any stochastic step records its seed in the recipe or the
ledger. A result you cannot reproduce is an anecdote.

**D4. Version everything that affects pixels.** Engine version, model versions and hashes, prompt
versions, profile versions - all stored in the recipe's provenance block. When a customer says
"this looked different last month", we must be able to prove exactly what changed.

**D5. Builds are reproducible from a tag.** Pinned toolchain, pinned dependency locks, pinned
`models.lock`. A release that cannot be rebuilt cannot be debugged.

**D6. Time and locale are inputs, not ambient facts.** No hidden dependency on the system clock,
timezone or locale in any decision path. Pass them in, so tests can control them.

**D7. Floating-point order is part of the contract.** Do not reorder accumulations for speed in a
colour-critical path without `COL` approval and a golden-image re-verification.

---

## Article V: Data rules

**V1. Migrations are forward-only and additive.** Never drop a column, never repurpose one, never
rewrite history. Every migration is tested by running it over a populated fixture catalogue with row
counts recorded before and after.

**V2. One owner per table.** Named in the data dictionary. Others read.

**V3. Provenance is mandatory.** Every derived value stores what produced it: model name, version,
confidence, timestamp. A score without provenance becomes garbage the first time a model changes.

**V4. Transactions wrap whole units of meaning.** A reader must never observe a half-analysed image.

**V5. Derived state must be recomputable.** Anything derived can be rebuilt from originals plus
recipes. If it cannot, we have created state that will drift silently and no one will notice for
months.

**V6. `user_edited_fields` is checked before every write.** This is Law 2 in implementation form and
it is worth stating twice.

**V7. Backup and restore are features, not hopes.** The catalogue can be backed up while in use and
restored to a working state. Tested every release, including restore into a newer application
version.

**V8. Personal data is minimised and scoped.** Face identities are per project and never global
across a photographer's catalogues without explicit action. Deleting a project deletes its
embeddings, its identities and its ledger.

**V9. No interactive full scans.** Every query the interface issues has an index and a measured worst
case under 50 milliseconds on a 4,000-image catalogue.

**V10. The ledger has a budget.** Six megabytes per thousand images. Explainability that bloats the
catalogue will be deleted by users, and then nothing is explainable.

---

## Article VI: AI and machine-learning rules

**M1. No model ships without a model card.** Purpose, architecture, training data with licences,
size, latency on three reference machines, quality gate, known failure modes, fallback behaviour.
No card, no merge. This is the most frequently skipped rule in the industry and the most expensive.

**M2. Quality gates are defined before training begins.** Written into the card up front, so nobody
gets to move the goalposts after seeing the results.

**M3. Confidence must be calibrated.** Raw network outputs are not probabilities. Fit calibration on
held-out weddings; expected calibration error must be 0.05 or better before any confidence is shown
to a user or used to authorise autonomy.

**M4. Fairness is a gate, not an aspiration.** Colour and skin-affecting models report error per
Monk-scale skin-tone bucket, with spread within 1.0 dE00. A model that serves one population well
and another poorly is defective and does not ship.

**M5. Heuristic baseline first.** Implement and measure the classical method. If the model does not
beat it meaningfully on fixture weddings, ship the heuristic. Many "AI features" are a well-tuned
threshold with a better reputation.

**M6. Evaluate on weddings, not benchmarks.** All gates are measured on the three fixture weddings,
sliced by scene, lighting, camera and skin tone. Report the worst decile, never only the mean.

**M7. Wedding-level splits.** The same ceremony must never appear in both training and evaluation
data. Verified programmatically; leakage invalidates every number in the report.

**M8. No silent model swaps.** Model versions are pinned by hash in `models.lock`. Any change reruns
the evaluation suite and the golden images. A model update is a release event, not a background
detail.

**M9. Every model can abstain, and abstention has a defined behaviour.** "I do not know" is a valid,
valuable answer. Forcing a decision at low confidence is how autonomous systems destroy trust.

**M10. Cloud reasoning is governed.** Six approved tasks, hard per-task caps, a global ceiling of 75
calls and USD 1.50 per 3,000-image wedding, 70 per cent cache hit rate, strict output schemas, prompt
versions recorded in the ledger, and a tested local fallback for each.

**M11. Never send image content or client-identifying data by default.** Derived features only,
unless the user has consented per project.

**M12. Generative pixels are disclosed.** Anything a model invented rather than corrected is recorded
in the recipe and can be disclosed to a client. This is an ethical obligation in wedding photography,
not a legal formality.

**M13. Training data provenance is recorded before ingestion.** Source, consent scope, licence,
permitted uses, deletion path. Retrofitting provenance is impossible; the moment data is mixed, it is
mixed forever.

---

## Article VII: Testing rules

**T1. The pyramid, and everything in its place.** Unit tests for logic, integration tests for stages,
end-to-end tests for flows, soak tests for endurance, golden images for pixels, perceptual review for
judgement. Each layer has an owner and a runtime target.

**T2. Zero tolerance for flaky tests.** Fixed or deleted within 48 hours. The moment engineers learn
to ignore red, the suite stops protecting anything and no process will restore its credibility.

**T3. No arbitrary waits.** Wait for a condition, an event or a state. A `sleep` in a test is a
defect in the test.

**T4. One reason to fail.** A test asserting eight things tells you nothing when it breaks.

**T5. Every bug becomes a permanent test.** Reproduce, automate the reproduction, then fix. A bug
without a regression test will return, usually at a worse moment.

**T6. Fixture weddings are the shared reality.** `hindu_night` 3,200 frames, `daylight_church` 2,400,
`nepali_reception` 2,800. Features are proven on real weddings, never on three convenient images.

**T7. Test the ugly paths hardest.** Corrupt files, truncated RAWs, full disks, killed processes,
missing models, no network, cancelled jobs, upgrades over old catalogues, wrong colour profiles. That
is where customers actually live.

**T8. Property-based tests for anything with invariants.** Recipe round-tripping, merge logic,
geometry maths, mask composition. Examples find the bugs you imagined; properties find the ones you
did not.

**T9. Fast feedback or the suite gets bypassed.** Pull request suite under ten minutes. Everything
heavier runs nightly with results visible by morning.

**T10. Never weaken an assertion to go green.** Escalate instead. A quietly relaxed threshold is a
permanent, invisible loss of product quality, and nobody will ever notice it happened.

**T11. Coverage is a diagnostic, not a target.** 80 per cent on core logic is expected, but a
meaningful test of a critical path is worth a hundred trivial ones. Do not game the number.

---

## Article VIII: Performance rules

**P1. Budgets are tests.** Every stage has latency, throughput, peak RSS and peak VRAM budgets,
asserted in CI on reference hardware. A budget without an assertion is a wish that will be missed.

**P2. Three reference machines define reality.** RTX 4070 laptop with Windows 11 and 32 GB, M3 Pro
MacBook with 18 GB, and an Intel integrated-GPU desktop with Windows 11 and 16 GB using DirectML.
A win on one and a loss on another is not a win. Budgets are met on the weakest machine that the
phase claims to support.

**P3. Never optimise without a profile.** Intuition about hot paths is wrong more often than it is
right. Attach the profile to the pull request or the optimisation is unreviewable.

**P4. Report distributions.** p50, p95 and worst case. The p95 is what a customer experiences on a
difficult wedding, and difficult weddings are the ones they remember.

**P5. The five per cent rule.** Any change degrading a budgeted metric by more than five per cent
fails the build. Re-baselining requires written justification approved by the Chief Architect.

**P6. Memory is a budget too.** An out-of-memory crash at image 2,800 of 3,000 is the worst possible
performance outcome. Peak RSS and VRAM are asserted, and every GPU kernel declares its ceiling and
tiles rather than allocating.

**P7. Cold start counts.** First run with an empty cache is what a new customer measures you by.
Benchmark it explicitly and separately.

**P8. Backpressure everywhere.** Bounded queues, sized from the hardware plan, never from a
hard-coded constant. A pipeline without backpressure is a memory exhaustion bug with good intentions.

**P9. Cancellation is a performance feature.** 250 milliseconds from request to all workers stopped.
A photographer who cannot stop a three-hour job will not start one.

**P10. Never trade correctness or colour for speed.** Escalate the trade-off. `COL` and `QAIQ`
outrank the stopwatch, permanently.

---

## Article IX: Security and privacy rules

**S1. The threat model is a living document.** Updated whenever the architecture or the data flows
change. Assets, adversaries, entry points, mitigations, accepted risks, and who accepted them.

**S2. Secrets live in the operating system keychain, nowhere else.** Not in configuration files, not
in environment variables, not in the catalogue, not in memory longer than the call that needs them,
never in a log or a crash report.

**S3. Default deny on network egress.** An allow list with a documented purpose per destination, and
a CI test that captures traffic and fails if anything unapproved appears.

**S4. Never log a file path.** Paths contain client names, venue names and event dates. Hash them or
omit them. This applies to logs, telemetry, crash reports and support bundles equally.

**S5. Face embeddings are treated as biometric data.** Local only, project-scoped, never transmitted,
deleted with the project - regardless of what local law currently requires of us.

**S6. Models and updates are signed, and verification works offline.** Signature, then hash, then
opset compatibility, then load. Any failure degrades to the documented fallback and writes a ledger
entry. No network call in the verification path.

**S7. Supply chain is gated automatically.** SBOM generated per release. `cargo-deny` and
`cargo-audit` in CI. GPL-incompatible and research-only components are hard-blocked. LibRaw is
dynamically linked and that is verified in CI, not assumed.

**S8. Vulnerability SLAs.** Critical within 48 hours, high within seven days, medium within thirty.
Tracked, reported per release, never quietly renegotiated.

**S9. Assume the support bundle will be posted publicly.** Design its contents on that basis. Users
can inspect it before it is sent.

**S10. Telemetry is counters and timings only, and it is opt-in.** No paths, no filenames, no image
data, no identifiers beyond an anonymous install identifier. Visible and revocable in settings.

**S11. Privacy is enforced by architecture, not configuration.** If a setting could leak client data
when misconfigured, the design is wrong. Defaults must be safe for a user who reads nothing.

---

## Article X: Reliability rules

**R1. Crash-free sessions at 99.5 per cent or better.** Measured across beta and production. Below
that, feature work stops until it recovers.

**R2. Every long job is journalled and resumable.** Write the intent, do the work, mark it done.
Recovery reads the journal; it never infers state by scanning the filesystem.

**R3. `kill -9` safety is verified, not assumed.** A scripted harness kills the process at multiple
points in every long pipeline, then reopens and resumes and diffs against a clean run. Zero
corruption is the only acceptable result.

**R4. Idempotency everywhere.** Re-running ingest, analysis, editing or export on the same input
produces the same state with no duplicates. Proven by running twice in CI.

**R5. Degrade, never collapse.** Missing model, missing GPU, missing network, missing optional pack,
full disk, unsupported camera - each has a defined reduced behaviour that still delivers a gallery.

**R6. Timeouts and retries are explicit and bounded.** No unbounded retry loops, no infinite waits,
exponential backoff with a ceiling, and every timeout value named and owned.

**R7. Disk space is checked before large operations.** Estimate, verify, and refuse politely with a
number rather than failing at 90 per cent through an export.

**R8. No resource leaks.** File handles, GPU memory, threads and sessions are all released on every
path including error paths. The 24-hour soak test is the proof.

---

## Article XI: Observability rules

**O1. Structured logs with a level, a target and context.** Never `println`. Never a bare string that
cannot be filtered or aggregated.

**O2. Trace identifiers flow through every stage.** One wedding run is traceable end to end across
the orchestrator, workers, inference and export.

**O3. The decision ledger is the product's memory.** Every automated decision records the decision,
the reason in photographer language, the confidence, the model versions and the inputs. This is what
makes autonomy defensible instead of frightening.

**O4. Errors carry actionable context.** What happened, what it affected, what the user should do
next. No codes, no stack traces in the interface, no "something went wrong".

**O5. Support bundles are one click and redacted.** Logs, hardware plan, model versions, timings,
counters - no image data, no paths, no client information.

**O6. Instrument before you need it.** Spans on every stage from the first implementation. Adding
instrumentation during an incident is how incidents become long.

**O7. Metrics are exported for the release dashboard.** Crash-free rate, stage timings, intervention
rate, cloud spend, model fallback frequency. If a number matters to the product, it is collected.

---

## Article XII: Version control and branching

**G1. Trunk-based development.** Short-lived branches, merged to `main` within two days. Long-lived
branches are where integration problems go to grow quietly.

**G2. `main` is always releasable.** Every commit on `main` has passed all gates. Broken `main` is a
team-wide stop-work event.

**G3. Conventional commits.** `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `chore`, `build`, with
a scope. Changelogs are generated, never hand-written.

**G4. Pull requests under 400 changed lines.** Larger ones are split. Review quality collapses beyond
that size and everybody knows it, including the reviewer who says otherwise.

**G5. CODEOWNERS is authoritative.** Colour paths require `COL`. Security paths require `SEC`.
Schema changes require `CTO`. Model paths require `MLL`.

**G6. Never mix refactoring with behaviour change.** Two pull requests. A reviewer cannot verify
behaviour inside a thousand-line reformat, so they will approve it without verifying.

**G7. No force-push to shared branches.** History on `main` is immutable. Revert forward.

**G8. Every pull request states its risk and its rollback.** One line each. If rollback is
"impossible", the change needs a feature flag.

---

## Article XIII: CI/CD and release engineering

**C1. Gates are mechanical, not social.** Build, lint, unit, integration, golden images, performance
budgets, licence and vulnerability scan, documentation check. A human cannot wave a gate through.

**C2. Every release is reproducible from a tag.** Pinned toolchain, pinned dependency locks, pinned
`models.lock`.

**C3. Rollback before rollout.** Never ship an update mechanism before the revert path is tested from
the previous version, preserving catalogues.

**C4. Staged rollout, always.** Five per cent, twenty-five per cent, one hundred per cent, gated by
crash-free rate at each stage.

**C5. Signed and notarised on both platforms.** Clean install with no operating system warning is a
release gate, verified on machines that never had the product.

**C6. Feature flags on every risky path.** Generative cleanup, Zero-Touch, cloud tasks, new models. A
kill switch that requires a release is not a kill switch.

**C7. Semantic versioning with meaning.** Major for a breaking recipe or catalogue format change,
minor for features, patch for fixes. Format changes require a migration and a written compatibility
note.

**C8. No manual release steps.** Automated, or a checked item in the runbook. Human memory is not a
release process.

**C9. Release notes are honest.** What changed, what to watch for, what is known broken. Customers
forgive known issues; they do not forgive surprises.

---

## Article XIV: Change management and ADRs

**N1. An ADR is required for:** technology choices, schema or format changes, node-order changes in
the render pipeline, autonomy policy changes, licence-relevant dependencies, anything expensive to
reverse.

**N2. ADRs are numbered, dated, and immutable.** Superseded by a new ADR, never edited into a
different conclusion. The reasoning at the time is the valuable part.

**N3. Every ADR records the options rejected and why.** Six months later, that is the section people
actually need.

**N4. Contract changes are announced before they are merged.** Affected agents get notice and time to
respond. A silent contract change is the fastest way to break parallel work.

**N5. Breaking changes need a migration and a deprecation window.** Two releases minimum for anything
customer-visible.

---

## Article XV: Definition of Ready and Definition of Done

### Definition of Ready - no work starts without all six

1. The user outcome is written in one sentence, in the photographer's language.
2. Acceptance criteria are testable, numeric where possible.
3. Contracts and interfaces are merged.
4. Dependencies are satisfied or explicitly stubbed with agreement.
5. Performance and quality budgets are stated.
6. The owner and the reviewers are named.

### Definition of Done - all twelve, every time

1. Acceptance criteria demonstrably met, with evidence attached.
2. Tests written at the right layer and passing, including negative and interruption paths.
3. All quality gates for the phase green on all three reference machines.
4. Performance budgets met and asserted in CI.
5. No new `unwrap`, `expect`, `panic!`, `any`, or swallowed error.
6. Errors typed, surfaced and actionable; all four interface states handled.
7. Determinism verified: two runs, identical output.
8. Cancellation and resume verified for any long operation.
9. Documentation and the glossary updated in the same pull request.
10. Telemetry and ledger entries added where decisions are made.
11. Handoff note written for downstream agents, with contracts and measured numbers.
12. Feature flag added if the change is risky, and rollback stated.

---

## Article XVI: Documentation rules

**W1. Documentation ships with the change, in the same pull request.** Documentation debt compounds
faster than code debt and is never paid down later.

**W2. One vocabulary, governed by the glossary.** If the product says "scene" and the documentation
says "segment", we have manufactured a support ticket.

**W3. Never document a feature you have not run.** Documentation written from a specification is
documentation that is wrong.

**W4. Limitations are as prominent as capabilities.** Stating that denoise is weaker below ISO 800
prevents more disappointment than any feature list creates.

**W5. Every error message has a documented cause and remedy.**

**W6. Write for the tired professional at midnight.** The answer in the first two lines. Nobody reads
documentation for pleasure.

**W7. Every phase produces a handoff note.** What was built, contracts exposed, measured numbers,
known limitations, what the next agent needs. This is how twenty-three agents stay coherent.

---

## Article XVII: Compliance, licensing and legal hygiene

**L1. Every dependency's licence is recorded and approved before it is added.** Automated in CI; the
SBOM is published per release.

**L2. Copyleft is handled deliberately.** LibRaw is LGPL and is dynamically linked. Static linking of
any copyleft library is blocked by CI, not by good intentions.

**L3. Research-only model weights and datasets are physically quarantined.** Separate storage,
separate manifests, and a CI check that no shipping model's manifest references them. This includes
using them to generate labels for a shipping model.

**L4. Training data requires recorded consent with a scope.** Training, evaluation, marketing,
redistribution - each explicitly granted or not, before ingestion.

**L5. Consent is revocable and deletion is real.** We can identify, remove and evidence the removal of
any contributor's data, and record which models were trained on it.

**L6. Client data is never used to train a shared model without opt-in.** The learning loop is per
photographer and local by default.

**L7. Every public claim must be substantiated by a measured number in the repository.** Marketing
sits downstream of measurement, never upstream.

---

## Article XVIII: Incident management

**I1. Severity definitions.** Sev 1: data loss, original modified, or client data exposed. Sev 2: a
wedding cannot be completed, or a widespread visible quality defect. Sev 3: a feature is broken with
a workaround. Sev 4: cosmetic.

**I2. Sev 1 stops all other work.** Immediately, for everyone relevant.

**I3. Mitigate first, diagnose second.** Roll back, disable the flag, restore the user. Root cause
comes after the customer is safe.

**I4. Blameless postmortems within 48 hours for Sev 1 and Sev 2.** Timeline, impact, root cause via
five whys, contributing factors, and actions with owners and dates.

**I5. Every postmortem produces at least one automated check.** If the only outcome is "be more
careful", the postmortem failed.

**I6. Customers are told the truth quickly.** What happened, what it affected, what we did, what we
changed. Reputation survives honesty; it does not survive discovery.

---

## Article XIX: Accessibility and internationalisation

**X1. Keyboard-complete.** Every action reachable without a mouse. Professionals cull with one hand
on the keyboard, and this is also the accessibility baseline.

**X2. Contrast standards on all chrome.** WCAG AA for interface text and controls. Image content is
exempt by nature; nothing around it is.

**X3. No colour-only signalling.** Confidence, status and warnings use shape, icon or text as well as
colour.

**X4. Respect system preferences.** Reduced motion, text scaling and high contrast are honoured.

**X5. Externalise all strings from day one.** Retrofitting internationalisation costs five times more
than doing it at the start. Our first markets include Nepal, India and Australia.

**X6. Never assume Latin scripts or Western name order** in names, dates or sorting.

---

## Article XX: Technical debt policy

**Q1. Debt is recorded when it is created.** A `TODO` with an owner and an issue link, or it does not
exist. Anonymous `TODO` comments are litter.

**Q2. Twenty per cent of every phase is reserved for debt, tests and tooling.** Not negotiable away in
good weeks, because good weeks are when it gets done.

**Q3. Deliberate shortcuts are documented as such.** What we did, why, what it costs, and what would
trigger fixing it. A recorded shortcut is engineering; an unrecorded one is a trap for a colleague.

**Q4. Leave the campsite cleaner.** Small improvements to code you touch are always welcome - in a
separate commit from the behaviour change.

**Q5. Debt affecting an invariant is not debt, it is a bug.** Anything touching originals, user edits,
determinism, privacy or licensing is fixed now, not scheduled.

---

## Article XXI: Metrics

**Delivery, reviewed weekly.** Lead time from ready to merged, deploy frequency, change-failure rate,
time to restore service, pull request size distribution, review latency.

**Quality, reviewed per phase.** Gate pass rate on first attempt, flake rate, escaped defects, bugs
without a regression test, crash-free rate.

**Product, reviewed monthly.** Intervention rate - the share of images a photographer changes after
Autopilot, target 8 per cent or less. Time saved per wedding. Keeper agreement. Retention. Weddings
completed with Zero-Touch.

**One rule about metrics:** a metric that nobody acts on is deleted. Dashboards nobody reads are a
tax on attention.

---

## Article XXII: Governance, decision rights and amendment

**Decision rights.** Each agent owns decisions inside its mandate, consults on adjacent ones, and
escalates across boundaries. The escalation ladder is: owner, then the two owners together, then the
discipline lead, then `CTO` for technical matters or `PM` for product matters, then the user for
anything affecting scope, budget or brand.

**Standing vetoes.** `COL` on skin and colour rendering. `SEC` on secrets, egress and client data.
`QAIQ` on perceptual regressions. `QAL` on missing or flaky tests and incomplete release verification.
`PERF` on unjustified budget regressions. `CTO` on cyclic dependencies, the offline guarantee,
unversioned formats and destructive edits. `PM` on overclaiming and on automation without an override.

**Vetoes are technical, not political.** A veto is written down with a reason and a condition for
release. It cannot be overruled by schedule pressure, only satisfied.

**Disagree and commit.** Argue with evidence, decide within a day, record the decision, then execute
as one team. Re-litigating a settled decision without new evidence is a discipline failure.

**Amendment.** Anyone may propose an amendment to this constitution as an ADR with evidence. `CTO`
ratifies. Amendments are versioned and dated so we can see how our standards evolved.

---

## Article XXIII: Rules for AI coding agents

Much of this codebase will be written by AI agents such as Claude Code. These rules are binding on
them and on the humans directing them.

**AI1. Read before writing.** Read `CLAUDE.md`, this constitution, the phase file and your agent brief
before producing code. Read the existing module you are modifying in full.

**AI2. One phase at a time, one concern at a time.** Never let an agent implement three phases in one
session. Context degrades, invariants get forgotten, and review becomes impossible.

**AI3. Contracts first, in a separate commit.** Types, traits, errors and schemas reviewed before
implementation. This is what allows several agents to work without collision.

**AI4. Never invent an interface that already exists.** Search the repository first. Duplicated
abstractions are the characteristic failure mode of AI-assisted development.

**AI5. Tests are part of the deliverable, not a follow-up task.** An agent that produces code without
tests has produced half a change.

**AI6. State assumptions explicitly and stop when blocked.** If a specification is ambiguous, write
down the interpretation and ask. Do not guess and continue, and never invent a threshold, a licence
or a benchmark number.

**AI7. Never fabricate a measurement.** Performance numbers, accuracy figures and model behaviour come
from a run, never from a plausible estimate. Fabricated numbers in a handoff note poison every
downstream decision.

**AI8. Respect the invariants absolutely.** No agent may write to an original, overwrite a user edit,
add a network call, introduce a dependency, or change a schema without the required approval - however
convenient it would be for the task in hand.

**AI9. Keep changes small and reviewable.** Under 400 lines. If the task is bigger, split it and say
so.

**AI10. Write the handoff note.** What was built, contracts exposed, measured numbers, limitations,
what the next agent needs. This is how a twenty-three agent team stays coherent across months.

**AI11. Prefer deleting to adding.** The best contribution is often a simpler design with fewer
moving parts. Complexity added by an agent is complexity a human will have to maintain.

**AI12. When rules conflict, escalate rather than choose.** Ambiguity resolved silently by an agent
is a decision nobody reviewed.

---

## Article XXIV: Enforcement

### What blocks a merge

- Build, lint or `clippy::pedantic` failure; any new `unwrap`, `expect`, `panic!` or `any`.
- Missing or failing tests; a flaky test; an assertion weakened without approval.
- A quality gate red, a golden image drifting beyond tolerance, or a performance regression over five
  per cent without written justification.
- A licence or vulnerability finding; an unapproved dependency; a new network destination.
- A schema change without a forward-only migration and `CTO` approval; a colour path without `COL`
  approval; a security path without `SEC` approval.
- Missing documentation, missing handoff note, missing model card, or a pull request over 400 lines.
- Any violation of the Ten Laws. There is no exception process for these.

### What blocks a release

- An incomplete release verification matrix from `QAL`, or a known unfixed flake.
- Any perceptual regression identified by `QAIQ`: plastic skin, halos, banding, artefacts on faces or
  hands, visible gallery inconsistency.
- Crash-free rate below 99.5 per cent, or a failed 24-hour soak.
- An untested rollback, an unsigned or unnotarised artefact, or an unpinned `models.lock`.
- A fixture wedding that does not complete offline.
- A known critical or high vulnerability, or an unpublished SBOM.
- Undocumented user-facing behaviour, or a release note claim the product does not demonstrably do.
- A red entry in the budget register without an approved justification.

### How exceptions work

There is exactly one exception mechanism: a written waiver, naming the rule, the reason, the risk, the
expiry date and the approving veto-holder, recorded in the repository. No verbal exceptions. No
"just this once" that is not written down. Waivers expire; they do not become custom.

---

## A closing word from twenty-five years of doing this

Every failed project I have witnessed failed for the same handful of reasons, and none of them were a
lack of talent.

They failed because the team could not say no to scope. Because nobody owned the boring layers, so
ingest and error handling were written twice, badly. Because the tests became unreliable and everyone
learned to ignore red. Because performance was left until the end, when it had become architectural.
Because one clever person made an undocumented decision that three people later depended on and
nobody could explain. Because there was no way to roll back, so releases became terrifying, so
releases became rare, so each release carried more risk than the last.

Everything in this constitution exists to prevent one of those specific outcomes.

The rules will occasionally feel excessive. Writing a model card when you already know the model
works. Journalling ingest when nobody has ever pulled the cable. Measuring skin tone accuracy per
bucket when the average looks fine. Insisting on a rollback path for an update mechanism that has
never failed. Each of those will feel like ceremony on the day you do it, and each one will one day be
the reason a wedding was not lost.

Three last things I want every agent on this team to carry.

**Be the person who says the uncomfortable thing early.** "This will not meet its budget." "This model
is not licensed for commercial use." "The metric passed but the skin looks like plastic." "I do not
understand this specification." Every catastrophic project failure I have seen was preceded by someone
who knew, and stayed quiet because the schedule was tight and the room was optimistic.

**Protect the customer's work above your own convenience.** Their originals, their manual edits, their
privacy, their clients' faces, their reputation with the couple who hired them. Every shortcut that
touches one of those is a shortcut we do not take, no matter what it costs us in time.

**Ship something small that works completely.** A product that flawlessly culls and edits a wedding
with six features will beat one that half-does thirty. Cut scope, never quality. The lean sixteen-phase
path exists precisely so that you have a real choice available when the schedule gets hard - take it
early rather than shipping thirty compromised features late.

Build it as if a photographer's entire business depends on it, because one day soon it will.

---

*AURA Engineering Constitution, version 1.0. Amend through ADR. Enforce without exception.*
