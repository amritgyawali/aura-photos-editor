# AURA-ML-5099 - The micro-retouch matrix file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No small fixes anywhere in the project, and a message asking them to restore the file or
reinstall.

## What actually happened

`MicroTable::parse` refused `crates/aura-retouch/config/micro_retouch.toml`. The refusal is
**whole-file and run-blocking**, as phases 15 to 20's config refusals are: half a matrix would
clean the ceremony against measured ceilings and the reception against nothing, and that
inconsistency is invisible in a delivered gallery.

The detail line names the key and the rule. The five families of refusal:

* **A ceiling was raised.** The file tried to set `teeth_max_luma`, `sclera_max`, `iris_max` or
  `flyaway_max_area_frac` above the constant `aura_core::contract::micro` owns, or
  `require_confidence` below its floor. A studio may lower a ceiling; nothing may raise one.
* **An opt-in operation was switched on by default.** `clothing.strap` or `clothing.crease` with
  `default_on = true`. Both are refused: a studio switches them on per project rather than
  inheriting them from a file nobody read.
* **A row has no written reason.** Every threshold here is a product decision, and one nobody can
  explain is a product quietly deciding how somebody should look.
* **A locus is implausible.** A chromaticity offset above half a unit is off the spectral locus
  entirely; the file has a decimal point in the wrong place, or has stopped expressing an offset
  and started expressing an absolute colour.
* **A scene limit is outside its range, or a row is missing.**

## What to do

1. Read the detail: it names the key and the rule.
2. If the file was edited deliberately, the change is being refused because it would widen a
   promise. That is the mechanism, not a bug - see `docs/retouch-ethics.md` section 4.
3. Restoring the shipped file always works: it is compiled into the binary and
   `MicroTable::embedded` is what a fresh install reads.
4. **Bump `version` on any change.** It is written into `micro_plan.matrix_ver`, and a plan made
   under one table is not comparable with one made under another - `AURA-ML-5096`.
