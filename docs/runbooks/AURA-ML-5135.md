# AURA-ML-5135 - A solved camera transform failed held-out verification

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

That camera was matched from what AURA knows about the brand rather than from this wedding, and the
per-camera report says exactly that.

## What actually happened

**This is the one error in the phase that means the product worked.**

Section 6.2 requires a solved transform to be checked against pairs the solver never saw. A quarter
of the verified matched pairs are held back before the fit, deterministically by a hash of the two
photographs' own ids, and the appearance distance is measured on them before and after. When the
correction does not improve them by at least five per cent, it is thrown away and the brand baseline
is used instead.

A build that never raised this would be a build whose check was not running.

## Why an overfitted transform is worth catching

The transform vector has ten free parameters. A fit on a handful of pairs can describe those pairs
beautifully and be wrong about the camera - and there is no way to notice that by looking at the
pairs it was fitted on. The failure it prevents is a systematic colour shift applied to every
photograph a body took, justified by nine photographs.

## Three states, not two

`CameraTransformDto.heldoutImproved` is `true`, `false` or **`null`**. The third is "there were
fewer than three spare pairs to check against", which is not a check that passed. A panel that
rendered `null` as a pass would be claiming a verification nobody ran.

## Fixing it

Usually nothing to fix: the fallback is correct and the report is honest. It is worth looking at
when it happens on a body with **many** verified pairs, because that suggests the pairs are not what
they claim to be.

Check `camera_pairs` for that body: pairs with a high `subjectSimilarity` and a
`backgroundAgreement` barely above the scene's floor are the ones to be suspicious of, because they
are the pairs whose two frames may not have been in the same light after all. Raising that scene's
`background_agreement` in `camera_match.toml` - which the loader permits, since it demands *more*
evidence - is the usual remedy.
