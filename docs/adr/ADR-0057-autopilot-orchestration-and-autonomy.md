# ADR-0057 - Autopilot orchestration, checkpointing and the autonomy gate

- **Status:** accepted
- **Phase:** 28
- **Supersedes:** nothing
- **Amends:** nothing frozen. Section 5's interfaces are implemented with two substitutions, both
  recorded in section 4 below.
- **Related:** ADR-0006 (waived phase-02 conditions), ADR-0027 (the decision ledger and
  confidence), ADR-0029 (the render pipeline and the absent `wgpu` backend), ADR-0050 (a cloud
  answer that can only make the product do less), ADR-0055 (QC and the re-edit loop), ADR-0058 (the
  autopilot IPC surface)

## 1. Context

Twenty-seven phases decided things about a photograph, a person, a camera body or a gallery. None
of them is worth anything to a photographer at one in the morning unless it runs as one reliable
job. This phase is the job.

The engineering in it is not the pipeline - that already exists - but what happens when a two-hour
run meets a consumer laptop: sleep, thermal throttling, a full disk, an unplugged drive, a driver
reset, and a person who closes the lid. Section 1 of the phase document says so, and it is right.

There is a second problem that the phase document names only in passing and that turns out to be
the harder one. Phase 13 built an autonomy system - four bands, a PM-owned threshold table, two
structural risk multipliers - and wrote in its own contract that the bands are "the only thing
standing between a calibrated number and phase 28 acting on it unattended". Phase 13 also shipped
with `calibration_ver = 0` and `uncalibrated_raises = true`, which moves every decision one band
toward review. So the headline feature of this phase arrives into a build where, by its own
predecessor's design, almost nothing is permitted to act quietly.

## 2. Decision

Build the orchestrator as a **scheduler that decides nothing**, and let phase 13's bands decide
what it may run.

Concretely:

1. Twenty-five stages, declared as a compile-time table with dependencies, scope, checkpoint
   granularity, optionality, a per-item estimate and resource needs.
2. Execution through three **ports** - `StageRunner`, `AutonomyGate`, `MachineProbe` - defined in
   `aura-jobs` and implemented in `aura-app`. `aura-jobs` depends on `aura-core` and `aura-catalog`
   and on none of the twenty-two deciding crates.
3. Checkpoints keyed by a hash of what each stage read, stored in the same transaction as the work.
4. A resource governor whose every action makes the product do *less*.
5. Degraded completion: an optional stage that could not run leaves a `CompletedDegraded` run with
   that stage named, rather than a `Completed` run that quietly did less.

## 3. Why a scheduler must decide nothing

The moment the orchestrator can express an opinion about a photograph, the product has two answers
to "why was this frame delivered", and one of them belongs to the scheduler. That is unfixable
afterwards, because nothing records which of the two a gallery came from.

So there is no field anywhere in `aura_jobs::contract::autopilot` that can hold a keep, a rejection,
a strength, a parameter, a threshold or a confidence about a frame. `crates/aura-jobs/tests/no_decisions.rs`
is the grep that keeps it true - the eighth grep-as-a-test in this repository.

The manifest is the first lock and the grep is the second. Both are needed: the manifest catches a
dependency, and the grep catches the version where somebody adds the dependency and the call in one
commit.

**`StageDecl` is named that rather than `Stage`** because this crate has carried a
`crate::graph::Task` since phase 01, and a bare `Stage` beside it reads as the thing a task belongs
to rather than as its declaration. Nothing else about section 5's shape moved.

## 4. The two substitutions in section 5's interfaces

Section 5 freezes `RunHandle` with a `watch::Receiver<RunProgress>` and a `CancellationToken`, and
freezes `RunSummary` with `qc: QcReport`. Both are implemented with a substitution.

**`RunWatch` instead of `tokio::sync::watch::Receiver`.** This product's pipeline is synchronous -
rayon and `parking_lot`, with `tokio` reaching only as far as the Tauri boundary - so a `tokio::sync`
receiver in the orchestrator's frozen signature would put an async runtime inside the one crate that
has to be drivable from a plain test. Section 10.1's whole chaos suite is plain tests, and it would
not be. `RunWatch` has the semantics the signature needed: cheap to clone, always readable, always
the newest value, with a version counter so a poller can tell a change from a repeat. The cancel
token is `aura_core::progress::CancelToken`, which has existed since phase 01 and is what every
other pass in the product already takes.

**`qc: Option<QcReport>` instead of `qc: QcReport`.** Phase 27 made `Outcome::Skipped` a variant for
exactly this reason: a run whose QC stage was switched off must not carry an empty report that reads
as a clean bill of health. An `Option` says "not checked"; a default-constructed `QcReport` says
"checked, nothing found", and those are different claims.

## 5. Why checkpoints are keyed by what a stage read

Section 6.1 asks for "a hash of stage inputs" that "detects when upstream changes invalidate a
checkpoint". The alternative - keying on time, on a run id, or on nothing - resumes happily onto
stale work, silently. A wedding whose scene profiles were re-tuned between two halves of a run would
deliver half a gallery graded one way and half the other, with every unit test passing.

Two things about the hash are worth recording because they will be re-argued.

**What it covers is deliberately narrow.** The stage's own identity, the orchestrator version and
the unit count - and *not* the run id, the wall clock, the machine, the governor's action or the
batch size. None of those changes what a stage would decide, and a hash that included them would
invalidate every checkpoint on every resume, which is a resume indistinguishable from a fresh run.

**What it does not yet cover is a real gap.** Each phase keeps its own analysis version -
`tone::ANALYSIS_VER`, `moments::GROUP_VER`, and twenty others - and none of them is exposed through
a frozen service, so the hash uses the catalog's schema version as a stand-in. That notices a
photographer importing more frames and a migration landing, and does **not** notice a scene profile
being re-tuned. Condition C5 of the exit report; until each phase publishes its own version, a
re-tune has to be followed by a fresh run rather than by a resume.

## 6. The autonomy gate, and what Zero-Touch honestly means on this build

`StageId::decision_kind` maps each stage onto one of phase 13's six kinds, or onto `None` for the
ten stages that measure rather than decide. A stage with no kind runs whenever its dependencies are
met and no gate is consulted - phase 13's rule that analysis is not a decision, as a scheduling
fact.

For the rest, `AppGate::band` asks `preview_band` what band a decision of that kind would get **at a
confidence of 1.0**. That is a *ceiling* rather than a prediction: a band that still needs review at
the most permissive input is a band no decision of that kind can beat. Each phase still bands each
of its own decisions with that decision's real confidence as it makes them; this gate decides only
whether the stage may run at all.

The mapping from a band to a verdict is `StageVerdict::from_band`, and one row of it is a judgement
rather than a reading:

| Band | Attended | Zero-Touch |
|---|---|---|
| `Auto` | act | act |
| `AutoZeroTouch` | hold | act |
| `Suggest` | hold | **act, and queue every decision for review** |
| `RequireReview` | hold | hold |

`Suggest`'s own words in phase 13 are "applied, and put in the review queue so somebody looks", and
`Autonomy::acts` returns false for it because what it forbids is a *silent* application. In an
attended session that means the product waits. In Zero-Touch it means the product goes ahead and
tells somebody.

This matters because of what `calibration_ver = 0` does. With `uncalibrated_raises` on, a reversible
kind lands on `AutoZeroTouch` and an irreversible one - retouch, curation, export - is raised twice
and lands on `Suggest`. Holding `Suggest` would ship a Zero-Touch button that does nothing at all on
every irreversible stage, on the only build that exists.

**So this build's honest Zero-Touch is: AURA does the work, and it asks about everything it cannot
take back.** The pre-flight says so in the photographer's own words *before* the run starts, the
panel says it again, and `AutopilotStatusDto.calibrated` is on the wire so neither can quietly stop
saying it. `RequireReview` holds in every mode and there is no switch anywhere in this phase that
changes that.

## 7. Why the governor can only make the product do less

`GovernorAction` is `Proceed | Reduce | Pause | Stop`. There is no variant that raises concurrency,
enlarges a batch or disables a check.

The consequence is that an unreadable temperature sensor, a machine on battery, a full disk and a
thermally throttled laptop all reach the *same* conservative state, and a governor that is wrong is
a run that is slow rather than a run that cooked somebody's laptop at one in the morning. Every
input is an `Option` and every `None` contributes `Proceed` - which sounds like the dangerous
default and is not, because `Proceed` is the absence of a restriction rather than a licence.

ADR-0050 gave phase 24's editorial judgement this property - `Decline | Stand | Unavailable`, no
`Approve` - so that an unreachable provider and a cautious model leave a photograph in the same
state. This is the same shape applied to hardware, and it is why the governor is safe to run on a
machine that exposes no telemetry at all. Which is this one: five of the seven readings are `None`
on this build, so the thermal, battery, memory and quiet-mode policies never fire. Condition C3.

**A full disk is the only reading that stops a run**, and that asymmetry is deliberate: a hot machine
cools, a busy foreground goes away, and a laptop gets plugged in, but a full disk stays full until
somebody does something about it.

## 8. Why a stopped run is continued and a delivered one is not

`RunStatus::is_resumable` is true for `Running`, `Cancelled` and `Failed`, and false for `Completed`
and `CompletedDegraded`. Pressing start on a stopped wedding continues that run; pressing it on a
delivered one mints a new run and redoes the wedding, which is what "run it again" means.

The first version of this treated every terminal status as final, which forced a resume to mint a
new run id - and because checkpoints are keyed `(run_id, stage)`, that found no checkpoints and
repeated every finished stage. Two hours of a photographer's evening, lost to a bookkeeping rule
rather than to a bug. The rule is enforced in two places: `RunStatus::is_resumable` in Rust, and
`autopilot_run_no_reopen` in migration 28, so it holds against a caller that never came through
`start`.

## 9. Alternatives considered

**Put the orchestrator in `aura-app`, where every pass already lives.** Rejected. The scheduler
would then be untestable without the whole application, and section 10.1's twenty-kill chaos suite
would need a process harness rather than plain tests. The ports cost one indirection and buy a
crate that a plain `cargo test` can drive to completion.

**Have `aura-jobs` depend on the twenty-two deciding crates and call their passes directly.**
Rejected for two reasons: it would be a crate that has to be rebuilt whenever any phase changes, and
it would be a crate every phase could reach back into. The second is the real one - what it would
eventually reach for is an opinion about a photograph.

**Reach each pass directly from `AppRunner` rather than through that phase's own IPC command.**
Rejected. Each command already owns the correct wiring for its pass - the preview service, the
inference engine, the store, the policy table, the cancellation registration - and a second copy
here would be a second place for it to drift. Calling the command also means the autopilot runs a
wedding through exactly the code path a photographer clicking each panel's button would run it
through; a scheduler with its own route is a scheduler whose results can differ from the panel's,
with nothing recording which route a gallery came from.

**Let `autopilot.toml` describe the stage graph.** Rejected. The file decides which stages a
photographer wants *run*; it cannot decide what depends on what. A stage list assembled at run time
is a stage list a studio could reorder into a wedding that graded before it culled.

**Remove a disabled stage from the graph rather than visiting and skipping it.** Rejected. Its
dependents would then look like stages with fewer dependencies than they have, which is how a build
ends up grading before it culls because somebody unticked a box. A disabled stage is visited,
recorded as `TurnedOff`, and unblocks its dependents.

## 10. Consequences

- `AutopilotService` is the twenty-fourth service of its kind and the first whose subject is a
  **run**. Phase 29 adds a curation stage to this DAG, phase 30 adds an export stage and reads these
  summaries as its learning signal. No phase may keep its own pipeline runner, its own checkpoint
  format or its own idea of what a finished wedding is.
- Migration 28 is frozen: four tables, one settings table, two views, three triggers.
- Six new error codes, `AURA-JOB-7004` to `AURA-JOB-7009`, in the `JOB` domain beside phase 01's
  cancellation, lease and retry codes rather than in the `ML` range every phase since 15 has used.
  Nothing in this phase is a model.
- Section 11's wall-clock budgets are **waived**, with an expiry condition: this machine has no GPU
  backend, no trained model and no camera file, so a run over a synthetic wedding measures the
  scheduler. Publishing a number that looked like a wall clock while being a fixture's would be
  worse than publishing nothing. What is measured instead is the orchestrator's own overhead, which
  is real on any machine: `autopilot_plan` and `autopilot_resume` in `perf/budgets.toml`.
