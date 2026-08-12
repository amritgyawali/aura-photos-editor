# Pull Request

## Phase

Phase: `NN - <title>`
Tasks completed: `<role codes and task names>`

## What changed

<Two or three sentences. What a reviewer needs to know before reading the diff.>

## Invariants

- [ ] RAW files untouched; all edits expressed as recipe changes
- [ ] `user_edited` fields respected and never overwritten
- [ ] Every new AI decision emits structured reasons into the ledger
- [ ] Determinism preserved (seeded, sorted iteration, no wall-clock branching)
- [ ] Local path complete without cloud; cloud path budgeted and cached
- [ ] No secrets, image content or personal data in logs or telemetry
- [ ] Feature flag and kill switch present for any new AI stage
- [ ] No identity-altering operation introduced

## Gates

| Gate | Before | After | Budget | Pass |
| --- | --- | --- | --- | --- |
|  |  |  |  |  |

## Tests

- Unit / property:
- Golden / perceptual:
- Integration on fixture wedding:
- Benchmark deltas:

## Screenshots or renders

<Before/after at 100 % zoom for anything that changes pixels.>

## Docs and ADRs

- [ ] Phase document updated if reality diverged from the plan
- [ ] ADR added for any architectural or contract change
- [ ] Model card updated if a model changed
- [ ] User-facing docs updated

## Reviewer hat

Reviewing role (must differ from implementing role): `____`
What I tried to break:
