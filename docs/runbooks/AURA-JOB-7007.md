# AURA-JOB-7007 - A stored checkpoint no longer describes work this build would do

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing goes wrong. A resumed run redoes one step it had already finished, and the run summary says
"Something this step depends on changed, so it ran again".

## What actually happened

`aura_jobs::checkpoint::invalidation` found that a stage's stored `inputs_hash` no longer matches
what its declared inputs hash to now, or that the project's unit count moved. Two causes:

* **`InputsMoved`** - something the stage reads has changed. On this build the hash covers the
  stage, the orchestrator version and the unit count, so this fires when a migration lands.
* **`ScopeChanged`** - photographs were imported or removed between the two halves of a run.

Both restart *that stage only*. Stages below it will then find their own hashes moved on the next
pass and restart on their own account, which is the correct cascade rather than an error.

## What this code never means

It never means a resume is unsafe. The opposite: this code exists because the alternative - trusting
a completed stage forever - resumes happily onto stale work, silently. A wedding whose scene
profiles were re-tuned between two halves of a run would otherwise deliver half a gallery graded one
way and half the other, with every unit test passing.

## Fixing it

Nothing to fix. If a resume redoes more than expected, `autopilot_reason` holds one
`stage_replanned` row per invalidated stage with the reason:

```sql
SELECT stage, code, detail FROM autopilot_reason
 WHERE run_id = ? AND code = 'stage_replanned';
```

Condition C5 of the phase 28 exit report is the known gap here: the hash does **not** yet cover each
phase's own analysis version, so a re-tuned scene profile is not noticed. Until it does, follow a
re-tune with a fresh run rather than a resume.
