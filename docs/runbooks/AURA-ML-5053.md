# AURA-ML-5053 - A must-have has no candidate photographs

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The coverage panel shows that part of the day as **missing**, with the words "no
candidates found". It is not shown as covered, and it is not filled with something else.

## This is usually correct

Not every wedding has a cake, a first look or a formal exit. The product cannot invent
coverage and must not imply it did. `Coverage::Missing` means no candidate existed at all -
never that the engine chose not to include one.

## The two cases that are not correct

1. **The frames exist but were never analysed.** Check `AURA-ML-5050` and
   `CullOutline::coverage` first; an unanalysed frame is not a candidate.
2. **The frames exist and are labelled as something else.** The rule matches on phase 07
   scenes and phase 10 interactions. A ring exchange classified as `ceremony` will not
   match the `rings` rule. Fix the scene label - it is a photographer-lockable field - and
   re-run the cull.

## Operator steps

1. Read which rules are missing from the coverage report.
2. For each, filter the grid by the rule's scenes and see whether frames exist.
3. If they do, correct the labels or record a keep override; both survive re-selection.
4. If they do not, deliver. The report is telling the truth.
