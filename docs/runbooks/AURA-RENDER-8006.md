# AURA-RENDER-8006 - An automated pass proposed a change to a parameter a person set

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## This is the system working

It is registered as an error so the refusal is **visible**, not because anything went
wrong. Section 6.4 of PHASE-14 and ADR-0029 section 6: a parameter a photographer touched is
never overwritten by an AI pass, a preset or the QC agent.

## What the photographer sees

A note after an automated pass: AURA left the adjustments you made exactly as they were and
applied its own changes only to the settings you had not touched. The Develop panel marks
protected controls with a dot.

## What it means mechanically

`aura_recipe::schema::merge` walked the proposal, found a path in
`provenance.user_edited_fields`, and skipped it. `MergeReport::refused` lists every path
skipped; the error context carries the same list and a count.

## Operator steps

1. The context's `paths` is the exact dotted list, e.g. `global.exposure,geometry.crop`.
2. To let an automated pass take a field back, the photographer resets it: `History::reset`
   with `ResetTo::AiSuggestion` removes the path from `user_edited_fields`. There is no
   flag, argument or setting that switches the protection off - by design.
3. If a pass is refusing on *every* field, check that it is not proposing a whole recipe
   built from defaults. `merge` compares values, so a proposal that repeats the current
   value on a protected path is not a refusal and does not raise this.
