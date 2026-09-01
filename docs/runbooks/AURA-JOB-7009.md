# AURA-JOB-7009 - A run is already going for this wedding, or the one named is already delivered

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

"This wedding is already being worked on. Stop that run first, or wait for it to finish."

## What actually happened

One of three things:

* **Another run is in flight for this project.** `idx_autopilot_run_one_in_flight` is a partial
  unique index, so two schedulers racing to start the same wedding end with one row and one error
  rather than with two runs writing the same stages.
* **The run named is already delivered.** `RunStatus::is_finished` is true for `completed` and
  `completed_degraded`, and a delivered run's record is what a photographer was told happened to
  their wedding. A correction is a new run, not an edit to an old one; the trigger
  `autopilot_run_no_reopen` refuses it in the database as well.
* **A project id on the IPC surface was not a project id.** This phase has no generic bad-request
  code, because every error in this product is a registered code with a runbook.

A *stopped* run - `cancelled` or `failed` - is not this error. Pressing start on one of those
continues it, because a checkpoint is keyed `(run_id, stage)` and minting a new id would find no
checkpoints and repeat every finished stage.

## What this code never means

It never means anything was lost or duplicated. The refusal happens before the second run writes
anything.

## Fixing it

Stop the run in flight, or wait for it. To see which run is open:

```sql
SELECT run_id, status, started_at FROM autopilot_run
 WHERE project_id = ? AND status = 'running';
```
