# AURA-ML-5108 - Face recovery was declined to keep somebody looking like themselves

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One or more faces left as they were, carrying `restore_identity_drift_skipped` in the panel with
the measured distance beside it, or `restore_strength_reduced_for_identity` where easing off was
enough.

## What actually happened

**This is the guarantee of phase 22, and it is registered so that it can be said out loud.**

Section 6.3 of PHASE-22 states it plainly: "This is the guarantee that the product never changes
what someone looks like." `face_recovery::enforce` renders the plan through the real renderer,
crops the face through the same 112 px two-point warp phases 06, 09 and 10 use, embeds the crop
before and after through phase 06 recogniser, and compares:

* below `MAX_IDENTITY_DRIFT`, the recovery stands;
* above it, the strength drops by `RESOLVE_STEP` and the face is rendered and measured again, at
  most `MAX_RESOLVES` times, reporting `restore_strength_reduced_for_identity`;
* still above after that, the face is **skipped entirely** and this code fires.

There is deliberately no fourth outcome. A face whose embedding has moved is not delivered at a
lower strength, because a face that has drifted a little is a face that has drifted.

The measured distance is stored on `restore_face.identity_drift` whether it passed or not, so
"no delivered face moved further than the ceiling" is
`SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0` rather than a sentence in a
document.

## What to do

1. Nothing. The photograph is unchanged, and that is the correct outcome.
2. `RestoreService::identity_refusals` lists every frame in the project this happened on.
3. A wedding where this fires on most faces is a wedding where the face-recovery head is
   mis-calibrated, not one where the constraint is too strict. On this build the head is
   untrained and never runs at all, so this code cannot fire; condition C2 of the exit report is
   what closes that.
