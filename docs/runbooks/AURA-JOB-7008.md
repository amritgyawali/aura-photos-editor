# AURA-JOB-7008 - The autopilot checklist and resource budgets were refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing starts. The Autopilot panel says AURA could not load the settings that decide what runs on
its own and how hard it may push the machine.

## What actually happened

`aura_jobs::policy::Policy::parse` refused `autopilot.toml`. It refuses, rather than falling back on
defaults, for the reason every policy loader in this product refuses: a table nobody can trust is
not a table to fall back from, because the fallback would be a set of numbers nobody chose applied
to somebody's wedding.

The refusals, in the order they are checked:

* The file will not parse as TOML.
* A checklist row names a stage that does not exist.
* A checklist row offers to switch off a **mandatory** stage - ingest, previews, embed or cull.
* A checklist row has no written reason. Every row in every policy table in this product carries
  one; a threshold nobody can explain is a threshold nobody can defend.
* The file **widens a bound the code owns**. Four of them are safety limits rather than preferences:
  `vram_ceiling` (0.80) and the two thermal ceilings (85 C, 95 C) may only be *lowered*, and
  `battery_floor` (0.30) and `disk_headroom` (1.6) may only be *raised* - both directions mean
  "more cautious".
* `thermal_pause_c` is at or below `thermal_reduce_c`, which would tell a machine to stop before
  telling it to slow down.
* `max_parallel_stages` or `batch_size` is zero.

## What this code never means

It never means a run was half configured. The refusal happens before any run is opened.

## Fixing it

Restore the file from the install, or fix the named row. `just phase-28-verify` exercises four of
the refusals against authored files and prints which code each produced.
