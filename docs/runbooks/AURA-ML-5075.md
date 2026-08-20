# AURA-ML-5075 - A profile could not be adopted or selected

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The Adopt button, or the per-project or per-chapter profile picker, reports that nothing was
changed. Whatever profile was in use stays in use.

## What actually happened

One of four refusals, and the error's `detail` says which:

1. **The profile does not exist.** A stale UI holding an id whose row was removed.
2. **The profile has not been adopted** and something tried to select it for a project. Only
   an adopted profile can be selected, which is what makes "adoption is an explicit action"
   (section 6.3) mean something.
3. **The engine string does not match.** A profile fitted against one render engine and applied
   by another has a measured dE00 about a build that no longer exists; see `AURA-ML-5077`,
   which is the *warning* version of the same fact. Adoption is refused; application is
   degraded.
4. **The chapter is not one of phase 07's nine.** A per-chapter override for a chapter that
   does not exist would be an override nothing could ever read.

## Operator steps

1. Refresh the profile list. Cause 1 is nearly always a stale panel.
2. For cause 2, adopt first and then select. The two-step is deliberate.
3. For cause 3, re-train the profile on the same archives. The scan is resumable and the pair
   fits are cached by content hash, so a re-train after an engine change is much cheaper than
   the first run.
4. Nothing is written on any of the four. There is no partial state to clean up.
