# AURA-JOB-7004 - The pre-flight found something that stops a run before it starts

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The Autopilot panel does not start. The pre-flight dialog stays open with at least one row marked
red, and that row says what to do about it in words rather than in a code.

## What actually happened

`aura_jobs::preflight::check` returned a report whose strongest verdict is `Block`. Four of the
eight checks can block, and each of them is something a person can fix in a minute:

* **The wedding will not open.** Its catalog failed to migrate. Open it once from the projects list,
  which runs the migration, then start the run again.
* **There are no photographs.** Import them first.
* **The disk cannot hold the output.** The row says how many gigabytes to free. The figure is the
  estimated output times `DISK_HEADROOM` (1.6), because a run also writes proxies, checkpoints and
  catalog pages, and a disk that fills at 90 % of a two-hour run is the most expensive failure this
  phase has.
* **A model a mandatory stage needs is not installed.** Install the model pack from Settings.

Everything else warns and lets the run start: unreadable hardware, a spent cloud budget, an
uncalibrated build and a laptop on battery all leave a run that still delivers something.

## What this code never means

It never means anything was changed. The pre-flight is entirely read-only - it counts photographs,
asks the filesystem how much room is left, and asks the runner which stages are available. Nothing
in it writes a row.

## Fixing it

Do what the blocking row says, then press start again. `just phase-28-verify` exercises all four
blocking conditions against fixtures and will show what each of them looks like.
