# AURA-ML-5093 - The crop rules table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No cropping at all, on any photograph, with the reason stated in the Geometry panel. Lens
corrections and straightening still run: they do not read this file.

## What actually happened

`crates/aura-geometry/config/crop_rules.toml` did not load. The loader refuses:

* a file that does not parse, or that names an unknown scene;
* a row with no `reason` - section 9 gives PM "approve `crop_rules.toml`", and a threshold
  nobody can explain is a product decision nobody made. The third config file in the product
  to enforce it, after `emotion_weights.toml` and `local_light.toml`;
* a `resolution_floor` below the contract's `RESOLUTION_FLOOR`, or an `improvement_margin`
  below `IMPROVEMENT_MARGIN`. **The file may only make the rules stricter than the
  contract, never looser** - a config file that could relax a safety guarantee is a safety
  guarantee that lives in a text file somebody can edit;
* a row that allows a face to be cut. There is no such field, so this cannot be expressed;
  the loader checks it anyway, because the cheapest place to catch a future field that
  could is the loader that would have to read it.

This is `run_blocking` rather than `warning` deliberately. A missing *row* is
`AURA-ML-5094` and falls back to leaving the frame alone; a missing *file* means no row can
be checked, and cropping every wedding to a default nobody approved is worse than cropping
nothing.

## What to do

Restore the file from the installation, or reinstall. It ships with the product and is not
written at runtime.

## How to confirm

```
cargo run --package aura-cli -- verify --phase 23
```

The gate loads the table and prints the per-scene rows it found.
