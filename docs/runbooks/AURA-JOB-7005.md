# AURA-JOB-7005 - A stage the wedding cannot be delivered without could not finish

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The run stops with "Could not finish" and names the step. Everything before that step is saved.

## What actually happened

One of the four mandatory stages - ingest, previews, embed or cull - failed
`MAX_STAGE_ATTEMPTS` times, which is three, with a doubling backoff from two seconds.

Every other stage in the pipeline is optional: an optional stage that runs out of attempts is
*isolated*, the run carries on without it, and the wedding finishes as `CompletedDegraded` with
that stage named. Only these four end a run, because a wedding with no import is not a wedding and
a wedding with no cull is four thousand unsorted files.

The underlying error is on the stage's row in `autopilot_stage.error_code`, and it is the code
worth chasing rather than this one:

```sql
SELECT stage, error_code, error_detail, attempts
  FROM autopilot_stage
 WHERE run_id = ? AND outcome = 'failed';
```

This code is also raised by two of migration 28's triggers, where it means something tried to
rewrite a delivered run's record or edit a recorded reason. That is a defect in a caller rather
than a failed run; the record was left as it was.

## What this code never means

It never means work was lost. Every stage commits its units and its checkpoint in one transaction,
so a failed run is a run that stopped - starting it again continues from the last committed unit.
`RunStatus::is_resumable` is true for `Failed`, and pressing start continues that run rather than
beginning a new one.

## Fixing it

Read the failed stage's own error code and follow its runbook. Then press start again: the finished
stages are not repeated.
