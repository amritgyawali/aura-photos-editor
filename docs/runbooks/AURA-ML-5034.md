# AURA-ML-5034 - A technical flag could not be dismissed

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence in the Integrity card, and the mark they tried to remove still there. Nothing else changed.

## What actually happened

`IntegrityService::dismiss` refuses in three cases, and all three are refusals rather than silent no-ops because a dismissal that appears to work and does not is worse than one that visibly fails.

1. **The photograph has no verdict.** Nothing has analysed it yet, so there is no flag to clear. Usually a frame imported after the last pass, or one that failed with `AURA-ML-5035`.
2. **The flag is not set on that frame.** Two panels open on the same photograph, one of them stale.
3. **`flag` is not a single flag.** The call takes one bit. Clearing "soft *and* noisy" in one statement would record one dismissal for two independent judgements, and the review history would not be able to say which one the photographer disagreed with.

## What AURA does automatically

Nothing. This is `ask_user`: the panel re-reads the verdict and redraws, so the second attempt is made against what is actually stored.

## Operator steps

1. `SELECT flags, user_reviewed FROM image_integrity WHERE photo_id = ?;` - `flags` is the stored `u32`; `IntegrityFlags::from_bits` names the bits.
2. If the row is missing, run the integrity pass for that project and try again.
3. If `flags` does not contain the bit, the panel was stale. Re-open the photograph.
4. Dismissing an exoneration - `intentional_motion`, `eyes_closed_ok`, `no_subject_detected` - is refused by design. Those are not penalties, and there is nothing to forgive.

## When this is not the problem

A dismissal that *worked* and then came back after a re-analysis is a different fault, and a real one: `user_reviewed` is checked inside the statement the re-analysis overwrites the row with, so the dismissal should survive. If it did not, that is a bug in `IntegrityStore::put`, not this code.

## Related

* `AURA-ML-5029` - the same shape for a refused grouping edit.
* `AURA-ML-5025` - the same shape for a refused chapter edit.
