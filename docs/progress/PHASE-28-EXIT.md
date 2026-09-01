# Phase 28 exit report - The Zero-Touch Autopilot

**Status:** implemented conditionally. Six conditions, two of them Sev 2.

Phase 29 may start. Nothing in it may claim a wall-clock result, an ETA-accuracy result or an
intervention rate until C1 and C6 close, and **nothing anywhere in the product may describe this
build's Zero-Touch as unattended without also saying that nothing is calibrated** while C1 stands —
which it does, on every build shipped so far.

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contract | `crates/aura-jobs/src/contract/autopilot.rs`, `RunId` in `aura-core`'s `contract/ids.rs` |
| Decisions | `docs/adr/ADR-0057-autopilot-orchestration-and-autonomy.md`, `ADR-0058-autopilot-ipc-surface.md` |
| The orchestrator | `crates/aura-jobs/src/` — the DAG, checkpoints, resume, governor, pre-flight, retry, summary, store |
| Schema | `crates/aura-catalog/migrations/0028_autopilot.sql` — four tables plus settings, two views, three triggers |
| Checklist | `crates/aura-jobs/config/autopilot.toml` — 25 rows, each with a written reason |
| IPC | `crates/aura-app/src/autopilot_commands.rs` — nine commands and the three ports |
| Panels | `ui/src/components/autopilot/` — six components, 42 tests, mounted in `App.tsx` |
| Gates | `tests/e2e/autopilot_chaos.rs` (12), `tests/e2e/autopilot_run.rs` (21), `crates/aura-jobs/tests/no_decisions.rs` (11) |
| Budgets | `crates/aura-perf/tests/autopilot_budgets.rs` (3), `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 28` |
| In the product's own voice | `docs/autopilot.md` |

**No model.** The eighth phase since 08 to ship none, and the reason is none of the three the
earlier seven gave. There is nothing here a model could do: the orchestrator's whole job is to
decide *what runs next*, and that is a topological order over a compile-time table. A phase that
trained something to schedule would be a phase that had given a scheduler an opinion.

**No cloud call.** Section 7 of the phase document asks for none and there is none. The stages can
reach a provider — phase 24's editorial judgement and phase 27's planner both do — and the run's
spend meter is the sum of what they spent, read rather than caused.

**One rule this phase is built on, and it is a negative.** `crates/aura-jobs/tests/no_decisions.rs`
is the eighth grep-as-a-test in the repository, and it fails the build if any type in `aura-jobs`
grows a field that could hold a keep, a rejection, a strength, a threshold or a confidence about a
photograph. The manifest is the first lock — this crate depends on none of the twenty-two deciding
crates — and the grep is the second, because the manifest catches a dependency and the grep catches
the version where somebody adds the dependency and the call in one commit.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | One button processes a complete wedding from RAW import to exported gallery | **Partly met** | 23 of 25 stages run end to end (`autopilot_run.rs`). Curation and export are `PhaseNotBuilt`: phases 29 and 30 do not exist, so **this build delivers no files**. Conditions C2 and C7 |
| 2 | Progress, ETA and spend are visible and accurate; completion notifies | **Partly met** | All three are on the wire and rendered. "Accurate" is unmeasured — section 10.1's ETA row needs a reference machine. Condition C4 |
| 3 | Killing or interrupting the app never loses work | **Met** | `twenty_kills_at_twenty_points_all_resume_to_the_same_finished_wedding`; every stage commits its units and its checkpoint in one transaction |
| 4 | Optional stage failures degrade gracefully with an honest summary | **Met** | `autopilot_run.rs` degraded suite and the gate's section 8: an optional failure plus two unbuilt phases finish as `CompletedDegraded` with all three named |
| 5 | Ten real weddings complete with intervention on 8 % of frames or fewer | **Not met** | Unmeasured. No real wedding exists in this repository. Condition C6 |
| 6 | Pre-flight catches disk, GPU, model and budget problems before the run starts | **Met** | Eight checks; four block, and the gate asserts each blocks. GPU and budget warn rather than block, deliberately: a run without an accelerator is slow and a run without cloud budget uses its local fallback, and neither is a reason to refuse to start |

---

## 3. Section 10.1 gates

| Gate | Threshold | Measured |
|---|---|---|
| A 3,000-image wedding end to end within budget | 2.5 h / 4 h | **waived** — C1 |
| Killing at 20 random points always resumes correctly | 20/20 | 20/20, all to the same finished wedding |
| Sleep/wake, unplug, disk full and GPU reset each recoverable | 4/4 | 4/4 as *governor rulings*; none of the four is a real device event on this machine — C3 |
| Optional stage failure yields `CompletedDegraded` with an accurate list | pass | pass |
| ETA within 20 % after 10 % of the run | 20 % | **waived** — C4 |
| Cancellation leaves no partial exports and no corrupted catalog | pass | pass; there are no exports to be partial, which makes half this gate vacuous — C7 |
| Intervention rate | <= 8 % | **unmeasured** — C6 |

Plus the assembly the gate proves and section 10.1 does not name: the DAG's order and its two
scope halves, the checklist's four refusals, the autonomy gate under all four bands, the governor's
one-way action, the retry budget, the storage bound and the IPC surface's three files agreeing at
229 = 229 = 229.

---

## 4. Conditions

**C1 — every stage in every measurement was a fixture. Sev 2.**
Section 11's four wall-clock rows are waived. This machine has no GPU backend (ADR-0029 section 4),
no trained model in any of the twenty-two deciding phases, and no camera file, so a run over a
synthetic wedding measures `ScriptedRunner`. What is measured instead is the orchestrator's own
overhead, which is real on any machine: 67 ms to plan and run 25 stages, 88 ms to resume.
**Closes with a GPU backend plus one trained model.** Until then no number from this phase is a
statement about how long a wedding takes.

The same condition covers the autonomy half, and that half is the one to be careful about: phase
13's confidences are unfitted, `uncalibrated_raises` moves every band one step toward review, and
this build's honest Zero-Touch is *"AURA does the work and asks about everything it cannot take
back"*. That is said in the pre-flight, in the panel and in `docs/autopilot.md`, and
`AutopilotStatusDto.calibrated` is on the wire so none of the three can quietly stop saying it.

**C2 — ingest and previews do not walk a card.**
Both stages count what is already in the catalog. The import wizard owns the walk, because it is
what knows where the files are; an autopilot run over an already-imported wedding has nothing to
walk, and one over an empty project is stopped by the pre-flight's `HasImages` row rather than by a
root this stage would have to invent. Closes when ingest is drivable from a stored source root.

**C3 — five of the machine probe's seven readings are `None`.**
`AppProbe::sample` fills disk free space and nothing else: temperature, host memory, video memory,
battery and foreground activity need platform APIs this product does not link. So the thermal,
battery, memory and quiet-mode policies are built, unit-tested and **never fire on this build**. The
governor is safe in that state by construction — every `None` contributes `Proceed`, and `Proceed`
is the absence of a restriction rather than a licence — but "AURA backs off before your laptop does"
is a promise this build cannot keep. `docs/autopilot.md` says so in the product's own words.

**C4 — every per-item estimate is a declared figure.**
`StageDecl::est_ms_per_item` comes from the phase document and was measured on no machine in this
repository, so the pre-flight's total and the ETA before the first measurement are quotations rather
than predictions. The panel refuses to render a time until this machine has measured its own
throughput — `ProgressPanel` shows "working out how long this will take" while `throughputPerS` is
zero — which is the honest half. Closes with a reference-machine run.

**C5 — `inputs_hash` does not cover each phase's own analysis version.**
It covers the stage, the orchestrator version and the unit count, with the catalog's schema version
standing in for the twenty-two per-phase versions (`tone::ANALYSIS_VER`, `moments::GROUP_VER` and
the rest), because none of them is exposed through a frozen service. So a resume notices imported
frames and a landed migration, and does **not** notice a re-tuned scene profile. Until each phase
publishes its version, a re-tune must be followed by a fresh run rather than by a resume.

**C6 — the intervention rate is unmeasured. Sev 2.**
Section 13's headline number — fewer than eight frames in a hundred needing a person — needs ten
real weddings and a photographer with an opinion. Neither exists here. **No claim about how much
work this product saves may be made from this build**, and the failure it would hide is the one this
phase makes easiest to hide: a run that finishes cleanly and delivers a gallery nobody would send.

**C7 — this build writes no files.**
Curation and export are `SkipCause::PhaseNotBuilt`, so a completed run leaves a chosen and edited
gallery in the catalog and nothing on disk. `AutopilotSummaryDto.outputPath` is empty and the panel
renders no folder rather than sending a photographer to look for files that are not there. Closes
with phases 29 and 30, and both slot into the existing DAG without changing a shape frozen here.

---

## 5. What is deliberately absent

- **No autonomy field anywhere on the surface.** `zeroTouch` is a boolean and what it unlocks is
  decided by phase 13's bands. A field that could name a band would be a field that routed around
  the only thing standing between a calibrated number and an unattended edit.
- **No command that runs one stage on its own.** A surface that could run the retouch without the
  cull could edit four thousand frames nobody is delivering. Every pass already has its own command
  from its own phase.
- **No way to reorder the pipeline.** The graph is a compile-time table; `autopilot.toml` cannot
  reach it and neither can the IPC surface. A stage list assembled at run time is a stage list a
  studio could reorder into a wedding that graded before it culled.
- **No stage removal for a disabled stage.** It is visited, recorded as `TurnedOff` and unblocks its
  dependents. Removing it would make its dependents look like stages with fewer dependencies than
  they have, which is how a build ends up grading before it culls because somebody unticked a box.
- **No `GovernorAction` that makes the product do more.** A sensor that is broken, absent or lying
  cannot cause anything worse than the run going at the speed it would have gone at anyway.
- **No cloud tie-breaker, planner or judgement of any kind.** ADR-0057 section 3.

---

## 6. Rollback

Every stage can be switched off from the checklist except the four a wedding cannot be delivered
without. The whole feature is off when nobody presses the button: no pass in this phase runs on a
schedule, on import, or on any trigger other than `autopilot_start`.

Migration 28 is additive — five new tables and nothing altered — so a downgrade drops them and
leaves every earlier phase's rows untouched. A run in flight when the app is killed resumes; a run
nobody wants again is simply never resumed, and its rows are the record of what happened.

---

## 7. Regression

The full previous-phase suite is green: `cargo test --workspace --all-targets` exits 0,
`cargo clippy --workspace --all-targets -- -D warnings` is clean, `scripts/check-banned.sh` is
clean, `cargo xtask contracts --check` reports 76 entries all locked, and the UI suite is 418 tests
across 34 files.

Phase 28 amends no frozen contract. It adds two — `crates/aura-jobs/src/contract/autopilot.rs` and
migration 28 — and re-locks two it changed by addition: `crates/aura-app/src/contract/ipc.rs` and
`ui/src/ipc/types.ts`, which gained eight DTOs and two inputs between them.

It also closed a lock gap of its own making before it could become one: **migration 28 was not in
`EXTRA_CONTRACTS`**. `docs/plan/CLAUDE.md` has listed every migration as a frozen contract since
phase 01, phase 16 found migration 15 missing the same way, and a migration whose three triggers are
the second layer under a Rust refusal is exactly the file where a digest matters.

---

## 8. Four things this phase got wrong first

**A terminal status is not the same question as an unfinished one.** `RunStatus` treated every
terminal state as final, so a resume had to mint a new run id — and because checkpoints are keyed
`(run_id, stage)`, that found no checkpoints and repeated every finished stage. Two hours of a
photographer's evening, lost to a bookkeeping rule rather than to a bug. `is_resumable` is now true
for `Cancelled` and `Failed` and false for the two delivered states, and `autopilot_run_no_reopen`
enforces the second half in the database. Phase 27 hit the same shape from the other side, where
`TicketStatus::is_open()` answered "is this outstanding" and was spent on "may automation still
act".

**A stage that re-planned its own inputs could not detect that they had moved.** `plan_stage` wrote
the freshly computed `inputs_hash` before the comparison read it, so every checkpoint always matched
and no stage was ever re-run. Every unit test passed, because each exercised one life. The general
statement is phase 19's, in a third place: **a value that has already absorbed the thing you are
testing for cannot be used to test for it.**

**A budget written before it was measured was wrong about the shape as well as the size.** The
storage note quoted a per-table breakdown of 4,050 B that nothing had measured. What
`dbstat` reports is 5,281 B, and — more importantly — the number does not grow with the wedding at
all, which is the first migration since phase 01 to have that shape. The budget is now measured, and
the **bound is asserted as well as the number**, by running the same orchestrator over ten times the
units. Phase 21 wrote this rule and phase 26 wrote its second half; this is the third application.

**A gate that reads a wall clock is a gate that measures the fixture.** The first version of the
phase gate printed the run's elapsed time beside section 11's budget, which on a `ScriptedRunner` is
a number that looks like a wall clock and is a measurement of the test harness. Four of section 11's
five rows are waived now, and the gate prints the six conditions it did **not** prove on every run
rather than leaving them in a document nobody opens.

---

## 9. Inherited conditions still open

Every Sev 2 from phases 05 through 27 is still open, and this phase is the one where they compound
rather than sit side by side. A run here executes twenty-two deciding phases in sequence, so the
gallery it produces is a placeholder embedding, ranked by an untrained expression head, culled on an
untrained aesthetic head, graded by a deterministic solver, and checked by a QC pass measuring the
result of all of them.

**That is the honest summary of this build: the pipeline is real, the plumbing is finished, and
almost nothing underneath it has been trained.** Phase 05's C10 is the root, and it closes for this
phase at the same time it closes for the twelve phases that read the embedding.

Phase 02's condition — the first real camera file reopens its criteria whatever phase is in flight —
is still the standing one, and it now reaches further than it did: an autopilot run is the first
thing in the product that would exercise every decode path in one go.
