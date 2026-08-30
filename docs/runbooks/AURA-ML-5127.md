# AURA-ML-5127 - Stored gallery rows came from different arithmetic or a different policy

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A note that AURA has improved how it matches a wedding together and is re-checking this one.
Anything they pinned, rejected, adjusted or switched off is kept.

## What actually happened

The stored nodes carry an `analysis_ver` or a `policy_ver` that is not this build's. Tenth
version-drift code in the product and the same shape as the other nine: comparing a row from one
version with a row from another returns a plausible number that means nothing, and this exists so
that comparison never happens silently.

## Two versions, and why there is no third

`analysis_ver` invalidates the tree, the anchors, the target and every delta, because all four come
from this build's arithmetic - the sub-clustering, the change-point statistic, the anchor ranking,
the robust statistics, the solver and the outlier threshold.

`policy_ver` invalidates every number that was compared against a **bound or a damping factor**,
because those are a product decision a release can move without changing a line of solver code. It
lives on `consistency.toml` and travels with the file.

There is no `model_ver`, because this phase ships no model. A column that can never change is a
column that will eventually be compared against and mean nothing.

## Why the whole project rather than a photograph

Unlike phases 09 to 24, this is a project-level check. A delta is a statement about a *node*, and a
node half-solved under one policy and half under another has a target that describes neither. So
`GalleryStore::is_current` asks about the project and a re-pass rebuilds the whole tree.

## Recovery

Automatic. The next pass rebuilds and the photographer's own decisions are carried across:
`take_decisions` reads the pins, rejections, overrides and switches out before the tree is cleared,
and `restore_decisions` writes them back onto whichever node each photograph now belongs to.
