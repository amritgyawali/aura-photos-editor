# ADR-0189: The automatic workflow takes the recommended choice after an idle minute

Status: accepted, 2026-10-29.

## Context

The full automatic workflow (ADR-0187) stops three times for a person after an import of several
photographs:

1. the Cull screen waits for **Edit the N best photos**;
2. when every photo is edited, the Auto edit screen waits for **Render final output**;
3. the Export screen waits for **Run export**, and for an output folder.

The photographer imports a wedding and walks away. They asked that a step nobody answers does not
hold the run: after one minute of inactivity AURA should pick the recommended option itself - for
the cull, edit the best set rather than wait for left-out photos to be brought back - and the same
at every other step.

## Decision

One timer, `useAutoChoice` in `ui/src/components/workflow/AutoChoice.tsx`, used at all three
stops. While a stop is waiting it counts one minute; any pointer movement or press, key, wheel or
touch in the window starts the minute again. When the minute runs out it does exactly what the
stop's primary button does, once. A line under the button says what will happen and when
("AURA will edit the 58 best photos in 0:42"), and **Let me decide** turns the timer off for that
step.

| Stop | Armed when | The recommended choice |
|---|---|---|
| Cull | the cull the import started has completed and the best set is not empty | edit the best set as it stands, with every photo the person already brought back or left out |
| Render | an edit run finished without being stopped, at least one photo is edited, and the Auto edit screen is on screen | go on to the Export screen; photos that failed are left, as the button leaves them |
| Export | the screen was reached from **Render final output** (by hand or by the timer) | export the finished photos with the chosen preset (Gallery by default) into the chosen folder, or into a new folder `Pictures\AURA Exports\<collection> <date time>` when none was chosen |

What the timer never does:

* It never runs on a screen the person opened themselves: a Cull screen without an import's cull,
  an Export screen opened from the sidebar.
* It never runs while the screen is busy (a cull, a choice being saved, an edit, an export), and
  never runs twice at one stop: once it has acted, or once the export has run, the stop is done.
* A failed export is not retried; it waits for a person, with the error on screen.
* It overrules no decision: everything chosen by hand before the minute ran out is what it acts on.

Finish Folder already runs end to end without stopping, so it needs no timer.

## Consequences

* An unattended import of several photos goes from import to exported files with nobody at the
  screen, about three minutes slower than a person clicking straight through.
* The default export folder is a new folder per run, like Finish Folder's, so an unattended run
  never writes into an earlier delivery.
* No IPC command or frozen contract changed. The default folder comes from Tauri's own path API
  (`pictureDir`), already allowed by `core:default`.
* Tests: `AutoChoice.test.tsx` (the minute, activity, decline, a new step), the Cull screen
  (edits the best set after a minute; not for a cull the person started; not after "Let me
  decide"), the Auto edit panel (renders only while on screen) and `DeliveryAutoExport.test.tsx`
  (default folder, the chosen folder, once a visit, never on a visit the automatic edit did not
  start).
