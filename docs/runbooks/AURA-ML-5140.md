# AURA-ML-5140 - The quality-control thresholds table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

AURA has not checked the finished photographs at all, and says so. Nothing was changed.

## What actually happened

`crates/aura-qc/config/qc_thresholds.toml` could not be read, could not be parsed, or asked for
something the contract does not permit.

The pass **halts** rather than falling back on defaults. That is deliberate and it is the same
choice phases 24, 25 and 26 made for their own policy tables: a QC pass running on thresholds nobody
chose would produce a report a photographer trusts and a set of remedies applied against numbers
from a file that failed to load.

## What the loader refuses

The file may **tighten** a bound and may never widen one. Specifically it is refused when:

* a scene row names a threshold larger than the code's own ceiling for that check - a studio may ask
  AURA to be fussier, never more permissive;
* `min_gain_share` sits below the contract's `MIN_GAIN_SHARE`, which would keep remedies that did
  not work;
* `max_collateral` sits above `MAX_COLLATERAL`, which would keep remedies that broke another check;
* `replace_confidence` sits below `REPLACE_CONFIDENCE_FLOOR`, which is the one bound in this phase
  whose mistake a photographer cannot see;
* `max_rounds` is above 2, or `max_tickets_per_image` is above 8;
* a scene slug is unknown, a category name is unknown, or a scene row is duplicated.

The direction is the point, and it is the shape phases 21, 22 and 24 use: a ceiling a studio can
lower and nobody can raise is what makes `docs/how-qc-works.md` a promise about the product rather
than a description of its defaults.

## Fixing it

The error's detail names the row and the bound. Restore the shipped file from the installation, or
edit the offending row to sit inside the contract's ceiling. `aura-cli verify --phase 27` loads the
table as one of its first checks and prints what was wrong with it.
