# ADR-0034 - The colour IPC surface

- **Status:** accepted
- **Date:** 2026-08-19
- **Phase:** 16 - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection
- **Extends:** ADR-0030 (develop IPC surface), ADR-0032 (tone IPC surface)
- **Deciders:** CTO, Senior Frontend Engineer, Colour Scientist, Product Manager

## Context

Phase 16 adds seven commands and thirteen wire shapes to a surface that already has more than
a hundred. `crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen
contracts, checked by `cargo xtask contracts --check`, so every addition needs an ADR and a
re-lock, in that order.

Five decisions in this surface are not obvious, and four of them are about what a panel is
*prevented* from doing.

## Decision 1 - Nine writable paths, and none of them is a white balance

`set_colour_override` can reach `global.contrast`, `highlights`, `shadows`, `whites`,
`blacks`, `vibrance`, `saturation`, `curve.points` and `hsl`. Phase 15's surface can reach
`global.exposure`, `global.temperature` and `global.tint`. **The two sets do not overlap, and
neither can reach the other's.**

That is section 2.2's boundary made structural rather than remembered.
`colour_commands::with_colour` is the only function in the module that touches a recipe's
parameters and it names nine fields; a tenth would have to be added by hand, in a function
with a doc comment that says why there are nine. `this_surface_cannot_reach_a_white_balance`
is the test, and `TonePanel.test.tsx` asserts the same thing from the other side by searching
the rendered panel for the words a temperature control would carry.

The consequence a reader should hold onto: the "warmer" alternative is a shift of the red,
orange and yellow bands, **not** a change of colour temperature. Warmth in phase 16 is a
grading decision; warmth in phase 15 is a statement about the light that was in the room.

## Decision 2 - The guarantee is on the wire as a measurement, not a badge

`SkinGuardDto` carries `maxHueShiftDeg`, `maxChromaChange`, `attenuation`, `resolves` and
`measured`. It does not carry `protected: boolean`.

A boolean would have been smaller and it would have been the wrong shape. Section 6.3's claim
is quantitative - at most two degrees and six percent - and a panel that says "skin protected"
is making a promise the photographer cannot check. A panel that says "skin moved 0.4 degrees,
and the limit is 2" is reporting one.

`measured` is the field that makes this honest in the other direction. **A frame with nobody
in it has no measurement**, and `measured: false` must not render as a perfect score - it
means the guarantee did not apply, which is a different sentence and a different colour of
text. `ColourStatusDto::skinMeasured` is the project-level version, and it is the number that
matters when it is low: a wedding at 100 % coverage and 3 % skin-measured has had its headline
guarantee verified on almost nothing, which is exactly what a build with an untrained face
detector looks like.

## Decision 3 - `worstSkinHueShift` is on the wire rather than derived

`ColourStatusDto` carries a `MAX` over the project. Every other figure in that shape is a
count or a mean.

It is there because it is the one number that **falsifies** the phase. A mean skin shift of
0.3 degrees over four thousand frames says nothing about the frame that moved two and a half,
and that frame is the one a photographer will find. Putting the maximum on the wire means a
support engineer can ask for it directly rather than paging through decisions, and migration
16 denormalises the column out of the guard document for exactly this query.

## Decision 4 - A variant is promoted through the same path as a hand-set value

`select_colour_variant` writes the promoted parameter set through
`ColourService::set_override` and then through `aura_recipe::schema::merge` with
`EditSource::User`. There is no second path and no "applied variant" state.

Two consequences, both intended. A promoted variant is **protected from the next automated
pass**, because choosing between three answers is authoring one. And a promoted variant is
undone the same way a hand-set value is - "reset to AI suggestion" - so a photographer has one
mental model rather than two.

The alternative considered was a `selectedVariant` field on the decision, with the renderer
resolving it. It was rejected because it would make the recipe depend on a catalog row to be
interpretable, and phase 14's rule is that a delivered file can be re-created from the RAW's
hash, the recipe, the engine string and the output spec - four things, none of them a
database.

## Decision 5 - The bands are readings, never regions

`BandReadingDto` carries an area, a hue, a saturation, a luminance and a confidence. It does
not carry a mask, a polygon or a crop.

The content bands are *inferred* from colour statistics rather than segmented (ADR-0033
decision 4), so there is no region to send. Sending one would mean inventing the evidence for
an adjustment that was made without it, and a panel that drew an outline around "greenery"
would be showing a photographer something the product does not know.

`confidence` is on the wire for the same reason. "AURA saw greenery and was not sure enough to
touch it" and "AURA saw no greenery" are different sentences, and only one of them is about
this photograph. `HslPanel` renders both.

## Consequences

- Seven commands: `colour_status`, `image_colour`, `colour_review_queue`, `accept_colour`,
  `set_colour_override`, `select_colour_variant`, `estimate_colour`. All seven run off the
  renderer thread, because all seven can touch SQLite and one of them grades a wedding.
- Thirteen wire shapes, all `camelCase`, all in `ui/src/ipc/types.ts` and
  `crates/aura-app/src/contract/ipc.rs`. `contracts.lock` is re-locked in the same commit.
- `CurvePointDto` is a pair of integers in the recipe's own 0-255 units, so a stored curve, a
  rendered curve and a drawn curve are the same numbers rather than three roundings of one.
- **The curve is validated on both sides.** `CurveEditor.isMonotone` refuses a non-monotone
  drag at the moment it happens, and `override_of` refuses the same set on the way in with
  `AURA-ML-5066`. Neither is redundant: the first is a photographer's experience and the
  second is what stops a script bypassing it.

## What this surface deliberately cannot do

- **Normalise a gallery.** Every value here is about one photograph. Phase 25 owns making a
  chapter agree with itself, and a command here that took two photo ids would be phase 25
  arriving early and without its coverage rules.
- **Return or set a mask.** Phase 18's.
- **Set a skin target.** There is no field for one anywhere in the thirteen shapes. The
  guarantee is a measurement of *movement*, so a target would have nothing to be a target for.
