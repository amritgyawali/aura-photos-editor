# AURA-ML-5126 - A scene has no gallery consistency policy row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing wrong. Photographs of that kind were matched with the most careful settings the table has,
and the Consistency panel says how many kinds of photograph have no guidance recorded.

## What actually happened

`consistency.toml` has no `[[scene]]` row for one of the 23 scenes in `SceneId::ALL`, so
`Consistency::scene` returned `ScenePolicy::neutral`: the lowest damping the range allows, the
widest tolerances, and no grade harmonisation.

The neutral row is deliberately the most careful in the table. An unargued scene is a scene nobody
made a product decision about, and the safe direction for one is to do less.

## Why this is a warning rather than a silent default

Phase 15 and phase 16 both publish `untargeted_scenes` for the same reason: a wedding matched under
the neutral row everywhere is a wedding nobody argued about, and that should be *visible* rather
than inferred from an outcome. `GalleryStatusDto.untargetedScenes` carries the list.

## What to check

    grep 'scene = ' crates/aura-brain-gallery/config/consistency.toml | wc -l

Should be 23. The bundled table covers every scene, so this error on a stock installation means a
studio's own file replaced it - and a studio's file replaces the bundled one wholesale rather than
merging into it, because a merge makes "what is this build using" a question with two answers.

## Fixing it

Add the row, with a written reason beside it. Section 9 makes the table a product manager's
deliverable, and every row in the bundled file carries an argument for its numbers.
