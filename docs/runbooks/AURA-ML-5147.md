# AURA-ML-5147 - A look could not be measured, selected or applied

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Their photographs are exactly as they were. This phase writes no recipe and moves no pixel of its
own, so a refused look leaves every frame rendered by whatever phases 15 and 16 decided - which is
phase 17's guarantee inherited rather than restated.

## What actually happened

One of these:

* **The look is not stored.** Usually a look somebody deleted in another window, or an identifier
  from a catalog that has been replaced.
* **The look has never been measured on any project.** `project_look_needs_a_match` is a database
  trigger and it refuses the selection. A look whose effect nobody has measured is a delta nobody
  has checked, and phase 22's rule is that a repair that cannot be measured is not performed. The
  panel does not offer the button until the number exists; this is the lock underneath that.
* **The project has no look selected** and something asked to rename or restrengthen one.
* **There were no analysed photographs to be a residual from.** A look is the difference between a
  page and what AURA would have done to *your* photographs, so a project phases 15 and 16 have not
  touched has nothing for the difference to be measured from. The look is still stored, with
  `LookCode::BaselineAbsent` on it, and it cannot be selected until a match exists.

## What to do

* Run Autopilot, or the tone and colour passes, so the wedding has a baseline. `LookStatusDto`
  carries both numbers - how many photographs can take a look and how many there are - and the card
  says which of the two is the problem.
* Measure the look again. Measuring is what produces the match figure, and the match figure is what
  makes a look selectable.
* If the look was deleted, measure it again from the same folder. A look is re-measured rather than
  repaired: the reference folder may have gained photographs and the measurer may have moved, and a
  partial replacement would leave buckets from one measurement beside buckets from another.

## What this error never means

It never means a photograph was changed and then changed back. Nothing in phase 31 writes a
recipe - `crates/aura-look/tests/no_recipe_writes.rs` is the grep that keeps that true - so there
is no half-applied state for a failure to leave behind.
