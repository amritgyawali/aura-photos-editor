# ADR-0058 - The autopilot IPC surface

- **Status:** accepted
- **Phase:** 28
- **Related:** ADR-0057 (autopilot orchestration and autonomy), ADR-0056 (the QC IPC surface),
  ADR-0027 (the decision ledger and confidence)

## 1. Context

Every command surface before this one answers a question about a photograph that is already on the
screen. This one has to answer, to somebody who pressed a button two hours ago and has come back:
what did you do, what did you not do, and why.

That changes what the shapes have to carry, and it changes what they must not.

## 2. The nine commands

| Command | What it is for |
|---|---|
| `autopilot_status` | The project header: runs, the newest one's outcome, whether the build is calibrated |
| `autopilot_preflight` | Eight checks before a two-hour job, each with a sentence saying what to do |
| `autopilot_start` | Start or continue this wedding's run |
| `autopilot_progress` | What the run in flight is doing right now |
| `autopilot_cancel` | Stop it |
| `autopilot_stages` | Every stage of the newest run, with what happened to it |
| `autopilot_summary` | What the newest finished run did |
| `autopilot_events` | Everything the governor asked for |
| `autopilot_set_settings` | Record what the photographer chose in the checklist |

`autopilot_start` returns as soon as the run is planned and the wedding continues on a worker
thread. A command that returned when the run finished would hold the IPC surface for two hours, and
the panel polls `autopilot_progress` instead.

## 3. Three shapes that carry more than they look like they need to

**`AutopilotStageDto.skipCause` and `skipText`, beside `outcome`.** A step that did not run means
several completely different things - the photographer switched it off, this release does not have
it, its model is untrained, or AURA is not confident enough to act unattended - and only the first
is fine. Folding them into `outcome` would make "skipped" one value, and the panel would have to
render a step the product could not do exactly like a step nobody asked for. Phase 27 made the same
distinction for one inspection; this is it one level up.

**`AutopilotSummaryDto.degradedStages` as a list of `(stage, reason)` rather than a count.** The
count is not what a photographer needs at one in the morning. The list is, and every entry carries
the sentence the orchestrator built for it - built in Rust rather than in the panel, so the summary
a photographer reads and the one a support bundle carries say the same thing.

**`AutopilotStatusDto.calibrated`.** On this build phase 13's confidences are not fitted, so every
band is raised one step toward review and Zero-Touch queues more than it eventually will. That is a
fact about the *product* rather than about the wedding, and without it on the wire the panel could
only show a photographer a review queue with four hundred frames in it and no explanation.

## 4. Why the progress stream is polled rather than pushed

Every other long-running command in this product streams progress over the Tauri event bus. This one
is polled, and the reason is the ports: `RunWatch` lives in `aura-jobs`, which has no event bus and
must not acquire one - it is the crate that has to be drivable from a plain test. `AppState` holds
the run's handle for the life of the run and `autopilot_progress` reads it, which is a poll every
half second against a `parking_lot::RwLock` read.

An event stream can be added later without changing any shape here: it would push the same
`AutopilotProgressDto`.

## 5. What is deliberately absent

**No autonomy field.** `AutopilotStartInput.zeroTouch` is a boolean, and what it unlocks is decided
by phase 13's bands. There is no field on this surface that could name a band, raise one, or grant a
stage permission the gate did not give it. Phase 21's rule: a ceiling can be lowered by a studio and
raised by nobody.

**No threshold and no per-stage strength.** The checklist decides which steps run. How hard each one
works is that phase's own decision, made through that phase's own config table, and a scheduler that
could scale it would be a scheduler with an opinion about photographs.

**No command that runs one stage on its own.** A surface that could run the retouch without the cull
would be a surface that could edit four thousand frames nobody is delivering. Every individual pass
already has its own command from its own phase, and those are where a photographer re-runs one step.

**No way to reorder the pipeline.** The graph is a compile-time table. `autopilot.toml` cannot reach
it and neither can this surface.

**No `output_path` input.** Where delivered files go is phase 30's decision. This build writes none,
`AutopilotSummaryDto.outputPath` is empty, and the panel renders no folder rather than sending a
photographer to look for files that are not there.

## 6. Consequences

- Nine commands, bringing the surface to 229 handlers, 229 definitions and 229 typed client
  wrappers, under an `autopilot` namespace in the client beside the twenty that came before it. The
  phase 28 gate asserts the three-way equality, as phase 27's does.
- Eight DTOs and two inputs in `crates/aura-app/src/contract/ipc.rs`, which is a frozen contract and
  is re-locked by this phase.
- Six React components in `ui/src/components/autopilot/`, with 42 tests. Five are props-driven and
  testable without a window; `AutopilotPanel` is the one that talks to the shell, which is the split
  phase 25 established and phases 26 and 27 followed. It is mounted in `ui/src/App.tsx`.
- **The panel polls one command rather than five.** `autopilotProgress` is the only thing that
  changes while a stage is working, and the other four are re-read once when the run ends - which is
  also when three of them are first written. A panel that patched its own state would show a
  photographer the run it predicted rather than the one that happened, which on a phase whose whole
  subject is what did and did not happen while nobody was looking is the one mistake worth
  engineering against.
