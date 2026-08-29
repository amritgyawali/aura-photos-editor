# AURA-ML-5094 - A scene has no crop rule row

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Some photographs delivered as shot, in a wedding where others were re-framed - and the
Geometry panel naming the scene that has no guidance. `GeometryOutline` lists the scenes.

## What actually happened

`crop_rules.toml` carries one row per scene in phase 07's 22-scene vocabulary, and a scene
with no row gets **the most conservative behaviour the product has**: the original framing is
kept, no variant is generated, and the frame is still levelled and lens-corrected. Invariant
7 says no threshold is global; the fallback is not a global threshold, it is the absence of a
decision.

The usual cause is a scene added to `contract::scene` without a matching row here. The
loader reports it rather than refusing the file, because one missing scene should not stop a
wedding being finished - that distinction is the whole difference between this code and
`AURA-ML-5093`.

## What to do

Add the row, with a written `reason`. `docs/geometry-and-cropping.md` explains what each
column means, and adding a row bumps `rules_ver`, which re-plans the affected frames
(`AURA-ML-5090`).

## How to confirm

`GeometryOutline::unpolicied_scenes` in the panel header, or:

```sql
SELECT scene, COUNT(*) FROM geometry_plan
WHERE  rules_row = 0 GROUP BY 1 ORDER BY 2 DESC;
```
