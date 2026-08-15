# AURA-ML-5028 - Stored moments were grouped under different versions

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, in the Problems list, while the moments view keeps working. Stacks stay on screen, splits stay split, and the counts stay correct - the grouping in the catalog is a real grouping, it was just made by a different build.

## What actually happened

`moments` carries **three** version columns and they invalidate three different things:

| Column | What it invalidates | Cost of a bump |
|---|---|---|
| `embed_ver` | every distance, and therefore every edge in the graph | the whole pass |
| `group_ver` | the candidate sweep, the scoring, the union-find, the size cap | the whole pass |
| `profile_ver` | the thresholds those scores were compared against | the whole pass |

All three cost the same to fix, which is why there is one code rather than three. What differs is *why* it happened, and the message names the numbers so a support engineer does not have to guess:

* **`embed_ver` moved** - phase 05 shipped a new embedding, or the perceptual pass was re-run at a new `MODEL_VER`. This is the common one, and it is also the one where the *result* changes most: a better embedding groups differently, which is the point of shipping it.
* **`group_ver` moved** - this build changed how grouping works. `graph::GROUP_VER` is bumped by hand for exactly this reason.
* **`profile_ver` moved** - `moment_profiles.toml` was edited, in the installation or in the shipped baseline.

## What AURA does automatically

Nothing is silently compared. The reader does not mix versions: a moment written under one tuple and a moment written under another are never averaged, ranked against each other or reported as one number. The stale count is logged once per distinct tuple, `MomentOutline` reports the **lowest** tuple present rather than the newest, and the next grouping pass rewrites every unlocked moment at the current versions.

Locked moments are **not** rewritten, and that is deliberate - a photographer's split is a decision about photographs, not about a model version. A project can therefore sit with a mixed set indefinitely, which is correct and is why the outline reports the lowest.

## Operator steps

1. Read the three stored numbers and the three current ones from the message.
2. `SELECT embed_ver, group_ver, profile_ver, COUNT(*) FROM moments WHERE project_id = ? GROUP BY 1,2,3;` - the same query `MomentStore::versions` runs. More than one row is the mixed state.
3. Re-run the grouping pass (`group_moments`, or `just phase-08-verify` on a fixture project). It is seconds for a wedding; section 11 budgets six.
4. If the mixed state persists, the remaining rows are locked. `SELECT COUNT(*) FROM moments WHERE project_id = ? AND user_locked = 1;` confirms it. That is not a fault.

## When this is not the problem

A photographer who says "the grouping changed" after an update is describing the *intended* consequence of an `embed_ver` bump, not this error. This code says the catalog holds two vintages at once; it does not say either is wrong.

## Related

* `AURA-ML-5015` - the same rule for embeddings.
* `AURA-ML-5018` - the same rule for face templates.
* `AURA-ML-5022` - the same rule for scene labels.

Four codes, one rule, four phases: comparing a number produced under one version with a number produced under another returns a plausible answer that means nothing, and the only defence is to make the comparison impossible rather than discouraged.
