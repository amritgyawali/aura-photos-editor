# AURA-ML-5067 - A tone, curve or HSL override was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A slider snaps back, or a curve edit does not stick, and nothing else about the photograph
changes.

## What actually happened

`ColourService::set_override` refuses in four cases, and all four leave the stored decision
exactly as it was:

1. **The photograph has no decision.** An override is a disagreement with something, and
   there is nothing here to disagree with. Run the grading pass first.
2. **The override is empty.** Every field is optional; an override that sets none of them is
   a no-op that would still mark the frame `user_edited` and stop every later pass from
   touching it. That is a trap, so it is refused.
3. **A value is outside `-100..100`**, or a supplied curve is not monotone. The second raises
   `AURA-ML-5066` first and arrives here as a refusal.
4. **The variant asked for does not exist.** `select_variant` shares this code: a frame whose
   punchier alternative was dropped because it would have clipped has no punchier variant to
   promote, and inventing one would promote a set nobody guarded.

## Operator steps

1. Check whether the frame has a decision at all - `image_colour` returning `null` is case 1
   and is the common one.
2. Check the value's range in the request. The panel clamps; a script calling the command
   directly does not.
3. Nothing needs repairing. A refusal here has written nothing.
