# ADR-0065: The one-click finish

Accepted 2026-09-12.

## Context

The product ships thirty phases of decisions and twenty-six services, reached through a
step bar, nine workspaces and sixteen panels. A photographer who just wants the
delivery - cards in, files out, no clicks between - had to drive six of those panels in
the right order, and nothing at the shell level joined them.

## Decision

Add `one_click_finish` to `aura-app`: a shell-level pipeline that runs, in order,

1. the ingest the wizard already started, waited through through `ingest_progress`;
2. `check_ai_key`, then `set_cloud_privacy` with the project switch on - the button's
   promise *is* the provider's judgement, so consent arrives with the press;
3. `autopilot_start` with `zero_touch`, waited through `autopilot_progress`;
4. `plan_geometry`, then `accept_geometry` over the review queue in bounded rounds;
5. `cull_project` at the default mode, so the gallery exists before anything expensive
   runs per frame;
6. `photo_auto_edit` (ADR-0064, unchanged) once per delivered frame, capped at
   `MAX_AI_EDITS = 600` frames so a run cannot produce a bill the photographer did not
   choose;
7. `export_run` with the shipped `gallery` preset, read-back verification on, to the
   destination the photographer chose.

Two contracts are amended, in this order, per the constitution: this ADR, then a
re-lock. `contract/ipc.rs` and `ipc/types.ts` gain `OneClickFinishInput`,
`OneClickFinishDto` and `OneClickStatusDto`.

## Why it is not a bypass

Every stage calls the command its own panel calls - phase 28's stage rule, one level up.
The pipeline accepts *proposals that already passed their guards* (a crop the safety
filter approved, an edit the merge protects), never a guard itself: hand-set recipe
fields still win, the spend governor still refuses unpriced calls, the export still
stops on a corrupt read-back, and automation still never chooses a destination - the
one field the button cannot fill.

A stage that cannot run is a note and a degraded continuation, not a dead button. If
preflight blocks the analysis the edit still happens; if the cull refuses there is no
gallery and every frame is delivered; if the key is dead every frame is graded by the
local reference and the row says so. The promise of the button is a delivery; the
promise of the `notes` array is that nothing about the delivery is hidden.

## Consequences

- `cargo test -p aura-app --test one_click` is the gate: import, run, poll, and assert
  written-and-verified files with a sealed manifest, offline.
- The progress row lives in the process, not the catalog: a restart forgets a finished
  row, and the sealed manifest remains the durable record of what was delivered.
- The status is polled, not pushed, like every other pass in the product.
