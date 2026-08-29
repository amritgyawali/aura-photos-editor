# AURA-ML-5119 - The cleanup policy table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No distraction removal at all, on any photograph, with the reason stated in the Cleanup panel.
Every other kind of edit still runs: nothing else reads this file.

## What actually happened

`crates/aura-generative/config/cleanup_policy.toml` did not load. The loader refuses:

* a file that does not parse, or that names an unknown scene;
* a row with no `reason` - section 9 gives PM and SEC joint ownership of this file, and a
  threshold nobody can explain is a product decision nobody made. The fifth config file in the
  product to enforce it;
* an `area_cap` **above** the contract's `AREA_CAP_DEFAULT`, a `denylist_overlap_max` above
  `DENYLIST_OVERLAP_MAX`, or a `zero_touch_confidence` below `ZERO_TOUCH_CONFIDENCE`. **The file
  may only make the policy stricter than the contract, never looser.** A config file that could
  relax a safety guarantee is a safety guarantee that lives in a text file somebody can edit;
* a row that removes a class from the denylist, or that marks a person-bearing class
  story-irrelevant. There is no such field, so it cannot be expressed; the loader checks anyway,
  because the cheapest place to catch a future field that could is the loader that would read it.

## Why `run_blocking` rather than `warning`

A missing *row* is `AURA-ML-5120` and falls back to removing nothing from that scene. A missing
*file* means no row can be checked, and tidying every wedding to a default nobody approved is far
worse than tidying nothing.

## The fix

Restore the file from the installation, or reinstall. The shipped copy is the one the phase gate
runs against.
