# ADR-0134: Twenty more effects - colour looks, light and lens, art, repair

Status: accepted. Date: 2026-10-20.

Twenty further tools in the shape of ADR-0131 and ADR-0132: each a `Tool` variant and an ordinary
saved retouch operation, rendered by `crates/aura-render/src/retouch_effects/` - `looks.rs`,
`lens.rs`, `art.rs` and `repair.rs` - never planned by an automatic pass, and held to the same
shared tests as the other forty-eight: nothing outside the selection moves, nothing changes at
zero strength, the same pixels on every machine.

| Colour looks (`looks.rs`) | Light and lens (`lens.rs`) | Art (`art.rs`) | Repair (`repair.rs`) |
|---|---|---|---|
| Split tone | Spotlight | Watercolour | Clipped highlight repair |
| Film fade | Rim light | Stained glass | Local dehaze |
| Bleach bypass | Chromatic aberration | Cartoon | Limbal ring |
| Teal & orange | Fisheye | Neon edges | |
| Colour splash | Anamorphic streak | Thermal | |
| Sky blue | Kaleidoscope | | |

None duplicates a tool already in the product: `main`'s measured Defringe and portrait-light
catchlights, the forty-eight effects, and the global Dehaze (local dehaze measures its haze on the
selection only) were each checked against.

## Decisions

- **The colour looks keep luminance where the look allows it.** Split tone, teal and orange,
  colour splash and sky blue move only the two colour differences, so a tint never lightens or
  darkens anything; film fade and bleach bypass change tone on purpose and say so.
- **Teal and orange holds skin.** The look exists to separate skin from its surroundings; a pixel
  near the skin hue is held at 85%, and a test asserts skin's hue moves less than 0.08 rad at full
  strength.
- **Sky blue moves sky, not skin or cloud.** Only bright pixels that are already bluish or washed
  out move, cloud tops above 0.9 barely do, and a test asserts a skin tone beside the sky is left
  exactly as it was.
- **Clipped highlight repair keeps the brightness and borrows the colour.** A highlight clipped in
  one or two channels (a yellow forehead, a cyan sky) takes the channel shares of the unclipped
  pixels around it, weighted by how unclipped they are; the luminance stays.
- **Local dehaze measures its airlight on the selection.** The dark channel and the 99.5th
  percentile haze level come from the selected area, so a misty background can be cleared without
  the global Dehaze changing the couple in front of it.
- **The remaps fall to nothing at the ellipse.** Chromatic aberration, fisheye and kaleidoscope read
  the frame elsewhere, weighted by `(1 - d^2)^2`, so like the liquify tools they never tear; fisheye
  is a warp (`Tool::is_warp`) and refuses an inverted selection.
- **Art edges are colour edges.** Cartoon ink and neon glow are measured on every channel and the
  largest taken: on the first test, an edge between a red and a blue of similar brightness drew no
  ink because the measure read luminance alone.
- **A sampled point is required where the tool cannot mean anything without one.** Colour splash
  joins clone, colour match, the sampled-skin tools, chroma key and local white balance in
  `Tool::needs_source`, which the recipe validator and the window both read.

## What this does not do

None of these tools finds its area on its own or creates picture content. Visual quality against
any other product has not been measured.
