# ADR-0188: The status bar says what AURA is doing and why a section is locked

Status: accepted, 2026-10-29.

## Context

The photographer reported that the edit sections sometimes "get stuck": no other section can be
opened and nothing on screen says why. They asked for one indicator that shows everything the
software is doing.

The sidebar is disabled by one expression in `App.tsx`, the OR of nine flags (folder run, Auto
edit, look matching, photo studio saving, export, import, reference analysis, file dialog, queued
import). The status bar from ADR-0123 listed the engine's jobs, but:

* it never said **which** of the nine flags was holding the sidebar, or for how long;
* the full edit (ADR-0187), the cull (ADR-0185) and the standalone advanced retouch, now the three
  longest operations on a photograph, did not register with the activity feed at all, so the bar
  said "Idle" while they ran;
* eleven IPC modules (`fullEdit.ts`, `advancedRetouch.ts`, `nativeRetouch.ts`, ...) called Tauri's
  `invoke` directly, past the one wrapper in `ipc/client.ts` that the audit log hangs off, so
  their requests were in no log and nothing could tell that the window was waiting on one;
* a flag that was never cleared kept every section shut for good. One such path exists: the folder
  run's handle is persisted in `sessionStorage`, and when its job is gone, `oneClickStatus` fails,
  the handle is kept and `automaticBusy` stays true.

## Decision

1. **Every request is tracked.** `ui/src/state/inflight.ts` records each request between sending
   and its answer, and keeps the last sixty that finished, with time taken and the engine's message
   when it refused. `invoke` in `ipc/client.ts` feeds it, and every IPC module now imports that
   `invoke` instead of Tauri's. High-frequency reads (`isQuietIpc`) are shown only when slow or
   failed. It is a plain module, not a store: the bar reads it on its one-second tick, so a grid
   scrolling a wedding does not re-render the bar.
2. **Every lock is named.** `App.tsx` builds the nine flags as a list of `UiLock`s with a label;
   `locked` is "any of them is active", the same as before. A disabled sidebar button's tooltip
   names the locks. The status bar lists them with how long each has been held.
3. **The three long operations register with the activity feed.** `full_edit::run` reports
   "Full edit: <file>" with "Stage n of 10: <title>"; `advanced_retouch::run` (not `run_keeping`,
   which the full edit also calls) reports its steps; the cull reports photos checked out of the
   total and its final step.
4. **The bar shows the whole picture.** A coloured light (idle, working, slow, stuck, unavailable);
   each job with its step, count, elapsed time, **time left** (from the rate this window has seen,
   after five seconds of movement) and a **stall warning** when neither its count nor its step has
   changed for a minute; the locks; requests still awaited. **Details** opens the locks, the
   engine's jobs with their last change, every awaited request with its command name and wait, and
   the last thirty requests and jobs that finished or failed. **Copy diagnostics** copies all of it,
   with the last 200 audit lines, as JSON for a bug report.
5. **A lock nobody holds can be released.** A lock is **stale** when it has been held for 15 s while
   the engine runs no job, no request is awaited and no request has started or ended for 10 s.
   The bar then says "Sections are locked by ..., but nothing is running" with **Unlock
   sections**, which clears every flag (and the folder-run handle). When work is still running the
   sections open by themselves; after 30 s the details panel also offers **Unlock anyway**, saying
   the work keeps running in the background. Every manual unlock is written to the audit log as a
   warning, with the locks and how long they were held.

## Consequences

* A stuck screen is now diagnosable from the screen: which lock, since when, and whether the engine
  is still doing anything or waiting on anything.
* Unlocking never cancels engine work; it only lets the photographer leave the screen. A component
  whose own busy state is stuck still shows it, but no longer traps the whole window.
* Thresholds (15 s stale, 10 s quiet, 60 s stall, 30 s slow request) are judgement, not
  measurement. They live as constants at the top of `StatusBar.tsx`.
* Tests: `ui/src/components/workflow/StatusBar.test.tsx` covers a named lock while work runs, the
  stale lock and its unlock, no stale call while a request is awaited, the history, stall and
  time-left, and the request tracker.
