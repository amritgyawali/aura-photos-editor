# ADR-0124: Body skin healed where the skin is, faster matte edges, and no colour added to a black-and-white photograph

Status: accepted. Date: 2026-10-16.

ADR-0123 section 10 ended with the one cost it could not remove: "The slowest exports are
full-length portraits: body-skin spot healing runs at a frequency radius of a few pixels over a
body rectangle of five megapixels or more, about 3.5 s a pass, two passes... Restricting that
work to the skin inside the rectangle, rather than the whole rectangle, is the next change."
This is that change, two smaller ones beside it in the same render, and a defect in the
finishing look found while checking which kinds of photograph the automatic edit handles badly.

## 1. Frequency healing runs on the skin, not on the body rectangle

A face is one patch of skin and the rectangle around it is mostly that skin. A standing figure
is not: the neck, two bare arms and two lower legs are a sixth of the body rectangle, and every
one of the sixty or so blurs a healing pass makes ran over the dress between them.

`retouch_clear::Layout` is the plane a pass works on and where its cells come from in the frame.
`islands` finds the separately selected parts of the selection on a grid of margin-sized cells,
each remembering the exact extent of the selected pixels in it, grows each part by the pass's
margin (fourteen frequency radii, the widest ring a candidate's surroundings are read in) and
merges any two whose margins overlap. `pack` lays the parts out in shelves, tallest first, with
an unselected gap of one margin between them, trying a few shelf widths and keeping the smallest
plane. The unchanged pass runs on that plane; the write-back maps each cell to its pixel. When
packing does not shrink the plane below 80 % of the whole rectangle - a face, or a body whose skin
is one connected region - the whole rectangle is used, exactly as before.

Why this is the same computation rather than an approximation of it: every quantity a pass
measures is either a mean weighted by the selection, to which an unselected cell adds nothing
whatever is in it, or is read within the margin of a selected cell, which the piece carries with
it. What can differ is the order a blur's running sums meet the gaps, which is rounding.

**The first version differed by 30 % at some pixels, and the reason is worth keeping.** Spots
straddling a patch's soft edge were healed in the whole rectangle and left alone in the packed
plane. The pass was right in both; the thresholds were not the same. A selection's robust spreads
(the darkness and redness limits, the pore relief) are medians over about 60,000 cells taken at
every n-th cell of the plane - and a different plane is a different every-n-th cell. On the test
figure the redness limit at the widest scale came out 0.015 in one and 0.007 in the other, a
threshold measured on a few thousand samples moving by more than packing moves anything else.
`Layout::sampled` now samples the packed plane at exactly the cells of the frame the whole
rectangle would have sampled, at full resolution and on the working grid alike, and for the whole
rectangle it reproduces the old every-n-th cell exactly. After that the two agree to a largest
relative difference of 2e-6, and `separate_patches_heal_as_they_do_in_the_whole_rectangle`
asserts under 1e-3 everywhere and under 1e-5 on average, with every unselected pixel bit-identical.

**A matte refined against the photograph leaves a haze.** The guided filter that moves a coarse
segmenter matte onto the photograph's own edges leaves coverage of 1e-5 to 1e-2 for a few dozen
pixels around the skin. Counted as selection, it joined both legs into one part and an arm into
the chest, and the packed plane was not small enough to use. A cell selected below 1/255
(`FAINTEST`) would move by less than one level in 255 if it were rebuilt in full, so it does not
join two parts; such a cell outside every part's margin is left exactly as it is, where the whole
rectangle would have moved it by less than that.

## 2. Two costs beside it in the same render

- **Matte edge refinement used one core for its box means.** Fourteen box means over the body
  rectangle, each through an integral image built on one core, were about 600 ms of the 900 ms a
  body matte took to render. They are the renderer's own `bands::box_mean` now: a running sum per row
  and per column, every core in use, the same result on every machine. It agrees with the integral
  image to about 3e-8 and is 2.3 times faster.
- **Every brush stroke swept the whole selection twice.** The automatic retouch covers a body with
  a dozen overlapping band strokes, and `Coverage::new` cleared and merged a plane the size of the
  whole body for each one. Each stroke is cleared, painted and merged over its own reach now -
  its points' box grown by its widest dab, pressure included. A stroke paints nothing outside it
  and a zero stroke changes neither a painted nor an erased value, so this is bit-identical, and
  `strokes_painted_over_their_reach_equal_strokes_swept_over_the_plane` compares every bit
  against the old sweep, erase strokes and heavy pressure included.

## 3. Measured

A 13.5 MP frame (3000 x 4500) with a standing figure whose body rectangle is 1200 x 4005 and whose
skin - chest, two arms, two lower legs - is about 15 % of it, healed by the body-spot operation
the planner writes (`FrequencyHeal`, radius 3 px, two passes) through
`retouch_tools::apply_with_mattes`: `crates/aura-render/tests/body_skin_islands.rs`, test `timing`.
Four-thread container, development profile (`aura-render` at opt-level 2), `AURA_PERF=1`.

| Step | Before | After |
|---|---|---|
| Frequency healing, two passes | 5.1-5.4 s | 1.85-1.92 s |
| Matte rendering with edge refinement | 880-970 ms | 620-680 ms |
| Brush coverage | 260-330 ms | 225-260 ms |
| Whole retouch stack for the operation | 6.3-6.7 s | 2.77-2.82 s |

Three runs of each, the "before" column with the renderer changes stashed and the same test.

The packed plane is 33 % of the body rectangle here (five pieces). The figure is synthetic, and the
gain depends on how much of the rectangle the skin is; a body whose skin is one connected region
(a sleeveless dress joining the arms to the chest) packs into one piece and gains only what
tightening its rectangle gives.

The global analysis was measured on the way and left alone: at proxy size the light-and-colour
plan takes about 40 ms and the finishing look about 15 ms; the two histogram passes that cost
about 180 ms each at 2048 px run on the 512 px thumbnail in the real flow.

## 4. A black-and-white photograph was given colour

`docs/automatic-editing.md` promises that "a sepia or toned black-and-white keeps its colour", and
`smart_edit::analyse_from` keeps it: a frame with no colour, or one hue throughout, gets no
vibrance and no white-balance correction. The finishing look (ADR-0123 section 1), which runs
after it, did not know. On a neutral black-and-white frame it added a split tone - warm highlights
6, cool shadows 3 on a frame whose warmth reads zero - which tints the print; on sepia it added
cool shadows to a warm print, and vibrance besides.

`GlobalPlan::monochrome` carries the planner's own judgement (`Monochrome::Neutral` for a frame
with no colour, `Monochrome::Toned` for one colour throughout - the same criteria the planner has
used for vibrance and white balance since ADR-0086, which measured the colour spread they rest on
at 0.057 for a sepia JPEG, 0.064 for a blue-toned snow scene and 0.14 or more for every colour
photograph in its set, against a threshold of 0.08), and `adaptive_look::plan` takes it. A
monochrome frame keeps the depth half of the look - a print benefits from an S-curve as much as a
colour frame does - and gets no split tone, no band saturation and no vibrance, with a sentence
saying so. `a_black_and_white_photograph_gets_depth_and_no_colour` and
`a_sepia_print_keeps_its_one_colour` run the planner and the look together and fail without the
change; `a_colour_photograph_still_gets_its_look` is the control.

## 5. The engine string is not bumped

The working grid of ADR-0123 changed healing output within a measured tolerance without bumping
`graph::engine_string`, and this follows it. Packing changes a healed value by rounding, and the
faint haze of section 1 by less than one level in 255; matte refinement moves a matte by about
3e-8. An 8-bit export of an unchanged recipe can therefore differ from one rendered before by one
level in a few pixels at the soft edge of a body selection. The look's change is not a render
change: it is a different recipe, which `AURA-RENDER-8007` already tells apart.

## What is not claimed

No real photograph was measured for this ADR: the container it was written in reaches no photo
source, and the 1,000 photographs of ADR-0123 are not in the repository. The folder-run total of
ADR-0123 has therefore not been re-measured, and the table above is a synthetic figure in a
development build. `crates/aura-app/tests/batch_profile.rs` with `AURA_PERF=1` on a folder of
full-length portraits is the measurement to make next. The monochrome criteria are the planner's
existing ones; they can call a colour portrait in front of a grey backdrop, with grey clothes,
monochrome, and the look now adds no colour to it, as the planner already adds no vibrance.
