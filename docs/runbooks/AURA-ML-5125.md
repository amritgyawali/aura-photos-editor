# AURA-ML-5125 - A photographer's gallery adjustment could not be recorded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

An adjustment in the Consistency panel did not take. The photograph is unchanged.

## What actually happened

One of four things:

1. **The photograph has no gallery delta.** It was never placed in a node - it has no chapter, or
   the pass has not run. `image_gallery` returns null for it, and a panel that rendered null as a
   zero delta would have offered an adjustment to a frame the pass never saw.
2. **The override asked for nothing.** Every field is optional and at least one must be present. An
   empty override that set `user_edited` would take a frame out of automation without changing
   anything about it, which is a state nobody could explain later.
3. **A value was outside its bound**, and it was **refused rather than clamped**. The five bounds
   are 450 K, 12 tint units, 0.35 EV, 8 contrast and 6 saturation, and they are enforced in the
   contract, in the service and again as CHECK constraints in migration 25.
4. **The pass is switched off for that frame.** A disabled frame may not carry a movement - the SQL
   refuses the row - so an override on one is refused rather than silently stored.

## Why a bound is refused rather than clamped

Phase 21's rule: a ceiling can be lowered by a studio and raised by nobody. A frame that needs to
move further than 450 K to match its chapter is a frame whose *per-frame* estimate is wrong, and
phase 15's own override is where that is fixed. Clamping would hide that: the panel would show a
value the photographer did not type, and the underlying wrong estimate would stay wrong.

## What this command does not do

It records the disagreement; it does not move a pixel. The pixels move when the develop panel
renders the frame and `aura_recipe::schema::merge` writes the same values, which is the only
function in the workspace permitted to write a recipe. Two writes rather than one, deliberately -
a service that could do both would be a second way to edit a photograph.
