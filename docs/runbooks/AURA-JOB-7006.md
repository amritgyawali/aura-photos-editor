# AURA-JOB-7006 - The resource governor ended a run to protect the machine

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The run stops with "Stopped, so nothing is lost", and the stage list shows the remaining steps as
"The run stopped before this step started".

## What actually happened

**A full disk.** It is the only reading that can stop a run, and that is deliberate: a hot machine
cools, a busy foreground goes away, and a laptop gets plugged in, but a full disk stays full until
somebody does something about it. Continuing would be writing until the write fails, which is the
failure this whole phase exists to avoid at 90 % of a run.

Every other pressure the governor can see reduces concurrency or pauses:

| Reading | Response |
|---|---|
| Video memory over the ceiling | halve the batch and the stage concurrency |
| Host memory over 85 % / 95 % | reduce / pause |
| Temperature over 85 C / 95 C | reduce / pause |
| On battery below the floor | pause |
| The photographer is working | reduce |
| The GPU stopped answering | reduce, and continue on the processor |

`GovernorAction` has no variant that makes the product do *more*, so a sensor that is broken,
absent or lying cannot cause anything worse than the run going at the speed it would have gone at
anyway.

## What this code never means

It never means the wedding failed. Everything committed before the stop is committed, and
`RunStatus::is_resumable` is true for a stopped run - pressing start continues it.

## Fixing it

Free some disk and press start again. `autopilot_events` and the `autopilot_event` table hold every
reading the governor acted on, newest first, which is where to look when a run is slower than
expected rather than stopped.
