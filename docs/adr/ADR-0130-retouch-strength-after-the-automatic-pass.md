# ADR-0130: Retouch strength sliders after the automatic pass

Status: accepted. Date: 2026-10-19.

The automatic portrait retouch measures each face and writes one operation per mark, line,
region and finish: on a portrait with heavy acne that is a hundred operations or more, each with
an `amount` it chose. The only way to make "the skin a bit less smooth" or "leave a little more of
the blemishes" afterwards was to find the right operations in the saved list and move them one at
a time, or to change one of the 52 settings and run the whole pass again. Evoto, which a
photographer is likely to be comparing against, puts a strength slider beside every kind of
retouch and lets it be moved after the AI has run.

## 1. A factor per category, stored beside the operations

`crates/aura-recipe/src/retouch_intensity.rs` holds one factor per `Category` - blemishes, skin
smoothing, texture, tone evenness, redness, light evenness, wrinkles, under-eye, eyes, teeth,
lips and makeup, shine, face light and contour, body skin, body blemishes, stretch marks, hair,
clothing, background, scene, other - under the recipe extension key `studio_retouch_intensity_v1`.
A category that is absent is at 100%, and 0..=200% is the stored range.

The factors are **not** multiplied into the operations. The operations keep the amounts the pass
measured, which gives four properties the alternative does not:

- 100% is always exactly what the pass decided, and moving a slider back restores it bit for bit
  (`tests/retouch_intensity.rs` asserts the rendered bytes are equal).
- Re-running the automatic pass rewrites its operations and leaves the photographer's sliders
  where they were. The factors live under their own key, and `schema::merge` records them as the
  photographer's edit.
- Nothing that plans or edits operations - the automatic passes, the advanced retouch QC, the
  operation list - can mistake a slider position for a measurement.
- Repeated moves never compound: 50% then 50% is 50%, not 25%.

## 2. Only the renderer reads scaled amounts

`retouch_intensity::effective(recipe)` is the stored stack with every automatic operation's
`amount` multiplied by its category's factor and clamped to 0..=1, and with the operations a
slider has set to 0% left out. The three render paths in `crates/aura-render/src/cpu.rs` call it;
nothing else does. `amount` is the blend weight in every operator (`retouch_tools.rs`,
`retouch_acne.rs`, `retouch_clear.rs`, `retouch_skin.rs`), so scaling it is a linear strength.

Leaving the zeroed operations out is not tidiness. The first version kept them at `amount = 0`,
and in the real window every slider at 0% rendered up to one level in 255 away from the photo
before retouch: a stack of operations that do nothing still goes through the retouch path, and
that is not bit-identical to no stack. The unit and integration tests had passed, on a 96 x 72
frame where the two paths happen to agree; the 10 MP portrait found it.

Two selection views read the stack differently. Saved-selection coverage shows where each saved
operation acts, whatever its slider says, so it reads the stored stack. The draft-selection
preview finds the operation being replaced by position, so it reads `scaled_stack`, which scales
every operation and drops none.

The factors are part of the recipe, so they are part of the recipe hash and the preview cache key:
a moved slider is a fresh render, never a stale cached one.

## 3. Which operations a slider scales

Only operations whose id starts with `auto-`: what an automatic pass wrote. A person's own
operations, and an automatic one a person has edited (which `native_retouch::edit` already renames
`manual-…`), keep their own per-operation strength control and are never scaled. The draft preview
of an edited automatic operation now renders it under its `manual-` id too, so the preview shows
the strength it will be saved with.

The category comes from the operation's name first and its tool second, because one tool does
several jobs - a dodge lifts an eye bag, a cheekbone and a sky - and the pass names each operation
for its job. Anything unrecognised falls into **Other**, so every automatic operation has a slider.
Categories follow the automatic retouch settings, so the slider a photographer looks for has the
name they set the pass up with.

## 4. How far up a slider goes

Above 100% a factor can only raise operations that are below full strength. `headroom` is
`1 / max amount` in the category, at most 2, and the panel's slider stops there; a category whose
operations are already at full strength stops at 100% and says so. A slider that moved nothing
would be a control that lies.

## 5. Sync

`sync` copies this photo's factors - 100% included, so a slider moved back is moved back
everywhere - onto every other photo in the collection that the automatic pass retouched, and skips
the rest. Each photo's own operations are untouched, so the same 70% blemish slider removes 70% of
*that* photo's measured marks. Every write is an ordinary undoable history step checked against
the recipe it was measured on (`save_if_current`), so a concurrent edit is never overwritten.

## What this does not do

It does not make any operation stronger than full strength, re-plan the retouch, or reach
operations a person made. It is not Evoto's algorithm, and nothing about Evoto's implementation was
used: the behaviour is taken from Evoto's public documentation of its sliders.
