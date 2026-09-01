# Phase 28 progress - The Zero-Touch Autopilot

One line per task, in the order section 8 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `feat/phase-28-zero-touch-autopilot`, cut and pushed before any code |
| CTO - ADR | `docs/adr/ADR-0057-autopilot-orchestration-and-autonomy.md` | - | Ten decisions; a scheduler that decides nothing, checkpoints keyed by what a stage read, a governor that can only make the product do less |
| CTO - ADR | `docs/adr/ADR-0058-autopilot-ipc-surface.md` | - | Nine commands; no autonomy field, no threshold, no per-stage strength, no way to run one stage alone |
| TLC - freeze | `crates/aura-jobs/src/contract/autopilot.rs` | 115 contract tests | 25 `StageId`s, `StageDecl`, `RunStatus`, `SkipCause`, `StageVerdict`, `GovernorAction`, `MachineState`, `PreflightCheck`, `RunWatch`, `RunHandle`, `RunSummary`, `AutopilotOutline`, `AutopilotOverride`, `AutopilotService`; `RunId` is the seventeenth typed id |
| SRC - migration | `crates/aura-catalog/migrations/0028_autopilot.sql` | catalog suite | Four tables plus settings, two views, three triggers; the event cap as a trigger and no free-text column |
| PM - checklist | `crates/aura-jobs/config/autopilot.toml`, `src/policy.rs` | policy tests | 25 rows with a written reason each; five bounds the code owns that a studio may only tighten |
| SRC - graph | `crates/aura-jobs/src/dag.rs`, `src/stages/` | unit | One deterministic order; a disabled stage is visited and skipped rather than removed |
| SRC - checkpoints | `src/checkpoint.rs`, `src/resume.rs` | unit | Keyed by a hash of what a stage read; `InputsMoved` and `ScopeChanged` restart exactly that stage |
| SRC - governor | `src/governor.rs` | unit | Seven readings folded with `max`; no variant makes the product do more, and only a full disk stops a run |
| SRC - preflight | `src/preflight.rs` | unit | Eight checks, four of which block; every row carries a sentence saying what to do |
| SRC - retry | `src/retry.rs`, `src/summary.rs` | unit | Three attempts with a doubling backoff; an optional failure isolates and the run finishes degraded |
| SRC - store | `src/store.rs`, `src/api.rs` | integration | One transaction per stage carrying the work and its checkpoint; `NewRun` bundles what a run is opened with |
| SRC - progress | `src/progress.rs`, `src/cancel.rs` | unit | `RunWatch` rather than a `tokio` receiver, so the orchestrator is drivable from a plain test |
| QAL - fixtures | `src/fixtures.rs` | - | `ScriptedRunner`, `FixedGate`, `FixedProbe` and six behaviours - a whole wedding without a photograph |
| QAL - chaos | `tests/e2e/autopilot_chaos.rs` | 12 | Twenty kills at twenty points, all resuming to the same finished wedding; cancellation, reopening and the two triggers |
| QAL - run | `tests/e2e/autopilot_run.rs` | 21 | The whole pipeline end to end, the autonomy gate under every band, degraded completion and the retry budget |
| QAL - grep test | `crates/aura-jobs/tests/no_decisions.rs` | 11 | The eighth grep-as-a-test: no keep, no rejection, no strength, no threshold, no confidence about a frame |
| SFE - IPC | `crates/aura-app/src/autopilot_commands.rs`, `contract/ipc.rs` | - | Nine commands; three ports implemented, and every stage arm is one call into the phase that owns it |
| SFE - state | `crates/aura-app/src/state.rs` | - | `register_run` / `run_of` / `finish_run`, so the panel can read a watch a worker thread is writing |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 229 handlers, 229 registered, 229 client wrappers - asserted by the gate |
| SFE - panels | `ui/src/components/autopilot/` - six components | 42 vitest | Checklist, progress, pre-flight, summary, the composition and the container; mounted in `App.tsx` |
| PERF - budgets | `crates/aura-perf/tests/autopilot_budgets.rs`, `perf/budgets.toml` | 3 | 67 ms to plan and run 25 stages against 5 s; 88 ms to resume against 20 s; the store's bound asserted as well as its size |
| CTO - gate | `crates/aura-cli/src/phase28.rs`, `main.rs`, `justfile` | - | Eleven checks plus the IPC parity count; exits 0, and prints the six conditions on every run |
| DOC - docs | `docs/autopilot.md` | - | One button, what stopping costs, what Zero-Touch honestly means on this build, and what this release cannot do yet |
| EM - registry | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-JOB-700{4..9}.md` | registry test | Six codes, six runbooks |
| EM - lock | `xtask/src/main.rs`, `contracts.lock` | contract check | Migration 28 added to `EXTRA_CONTRACTS`; 76 entries locked |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Plan and run 25 stages | 5,000 ms | 67 ms |
| Resume after a kill | 20,000 ms | 88 ms |
| Store, per 1,000 images | 10,000 B | 1,760 B |
| 3,000 images end to end | 2.5 h / 4 h | **waived** - every stage here is a fixture |

The two overhead rows are three orders of magnitude inside their budgets, and that is a property
rather than an optimisation: both are bounded by the **stage count** rather than by the photograph
count, so a 6,000-frame wedding resumes as fast as a 300-frame one. The budget keeps its full
headroom for the day a stage's own work is real, because the number it bounds is the scheduler's
share of a two-hour run and the scheduler is the only part of this that is not a fixture.

## What did not happen

Section 11's four wall-clock rows, section 10.1's ETA-accuracy row and section 13's intervention
rate all need a machine with a GPU backend, a trained model and a real wedding. None of the three
exists in this repository. They are conditions C1, C4 and C6 of the exit report rather than numbers.
