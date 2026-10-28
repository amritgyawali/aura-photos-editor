# ADR-0184: Fixes from the final tool-by-tool test

Status: accepted, 2026-10-27.

## Context

`docs/final-test-report-2026-10-27.md` (PR #82) tested every tool in the real window and on real
Canon CR3 wedding portraits, and listed thirteen defects with their causes. This ADR records what
was changed for each, what turned out on closer reading not to be a defect, and what this build
cannot fix. Every change was found on a real photograph, is held by a test that fails on the old
code, and was checked again in the window (`H:/aura-final-tools/verify_fixes.py` on the frames the
test found it on).

## 1. One-click delivery no longer withholds crowd photographs

The portrait preflight (`portrait_preflight.rs`) required a complete contour reading for every
face saved in the recipe. A guest at the back of a crowd, too small to measure, withheld the whole
photograph - three of thirteen in the test (a temple crowd, a night procession, a stage panel).

Two changes, and the window showed both were needed:

- A face is *required* only when an enabled automatic operation names it
  (`auto-portrait-v1-{face}-`); scene-wide operations name none. A face nothing touched is still
  measured when it can be, because a seam beside it is still a seam; when it cannot be, it is
  recorded in the report's new `uncheckedFaces` and holds nothing back.
- Automatic retouch does retouch small guests, so the first change alone delivered nothing more:
  the retouched faces were themselves too small for the screen-size check (it needs 20 px between
  the eyes). The preflight now renders at the size the advanced retouch's own quality control
  already used (`qc_level`, through the shared `qc_render_for_faces`): large enough that the
  smallest retouched face reaches 24 px, up to full resolution. The guest is checked, not refused.

The three crowd photographs are delivered (0 withheld, 3 files written and verified).

## 2. Dehaze in the editing preview

Already fixed on `main` by the time this branch was cut (`graph.rs`, ADR-0126's frame-wide base made
it cheap). `Stage::is_heavy` still listed Dehaze, describing a skip that no longer happens; it is
removed from that list.

## 3. Colour tools that act only on what they are named for

Three hand-placed tools changed every pixel in their ellipse, and an ellipse always holds more than
its target:

- **Teeth whitening** added the yellow excess to blue on every pixel; on a lip, "yellow" is its red,
  and lips turned magenta (three faces, blue +9 to +15 levels). Each pixel is now weighted by how
  much it looks like a tooth - green close to red, not strongly saturated, not dark - and blue is
  never pushed past neutral.
- **Red-eye** admitted red-brown eyeshadow (`B >= 0.5 G`) and turned it olive. It now acts only on
  red with almost no green or blue (the larger of the two under about 0.12 of red), as retinal red is.
- **Eye redness reduction** now acts only where blue is close to green - the white of an eye and a
  red vein on it - so brown eyeshadow and skin keep their colour.

All three are thresholds on ratios of the pixel's own channels, so they hold at every exposure and
every skin tone, and none compares a person against a fixed skin colour.

## 4. Sky is checked on the pixels as well as the shape

The sky network's answer was accepted on geometry alone: it reaches the top and stops above the
people. Dark trees behind a seated bride (33 % of the frame) pass that. A sky must now also be
bright for its frame or bluish (dusk and night skies are blue), and smooth next to the rest of the
photograph (`ai::sky_coloured`). The lighthouse's clear sky is still found (34.4 %, as before); the
trees and a top-down sea are not.

The stage photograph's 24.5 % is unchanged, and stays so on purpose. What the network takes there is
the banner's plain white areas and a black top corner that may be real night sky over an outdoor
stage; measured, its interior is as smooth and as free of sharp edges as the lighthouse's sky (mean
step 0.003 against 0.005, no step above 0.04). Nothing measurable separates it from an overcast sky,
and a rule tuned to refuse it would refuse real skies.

## 5. Light effects reach the lights of an ordinary photograph

Anamorphic streaks and bokeh highlights started their "reaches dimmer lights" slider above 1 in
linear light - 1.25 and 1.0 at the defaults - so on a display-referred photograph, where nothing is
brighter than 1, they did nothing at their default settings. Their unit tests passed with highlights
of 8 and 10. Both now use `retouch_effects::light_threshold` (0.95 at 0 %, 0.65 by default, 0.35 at
100 %), with tests at a highlight of 1; sparkle is capped below white. Sun rays and god rays were not
changed: they add light, and added light is invisible on a white neon sign, which is where the test
had placed them.

## 6. Sampled-skin tools change the skin

Skin smoothing, even sampled skin and skin dodge and burn read a wide band three radii out, and their
edge-shading correction takes back the difference near the selection's edge. A selection narrower
than the wide band - a cheek in a group photograph at the default radius - was all edge, and the tool
changed **nothing at all** (0.03 % less blotch energy in the test, 0 % of the cheek in the window). The
band radius is now kept to a sixth of the selection's shorter side (`retouch_skin::apply`); the same
cheek loses 14 %. `REFERENCE_VERSION` moves to `robust-patch-v4-band-inside-selection`.

The window's default sample tolerance moves from 0.08 to 0.15: 0.08 matched only a quarter to two
fifths of a real cheek, its own shading and blush falling outside it. `connected` still keeps a
skin-coloured background the skin does not touch out of the selection.

## 7. Dependent sliders say what they wait for

The eyeliner wing draws nothing without a liner, and a light's direction and height move nothing
until there is a light - by design, and the test read each as broken. The Portrait studio now shows
the wing, light direction, light height and relight direction disabled, with the reason, until their
parent is set.

## 8. The false face on clasped hands, and the groom in sunglasses

Both were seen through the Haar parse (`aura-portrait`), which the analysis view, the Portrait
studio and the face picker use. Clasped hands passed its weakest branch (5 cascade windows, one eye
confirmed, confidence 0.76) on 989A3150; the same evidence describes a real older man in 989A2925 (6
windows, one eye, 0.78), so no threshold on that detector separates them, and none was added.

Automatic retouch was never at risk: it detects with YuNet (`aura-vision::portrait`), which on the
same frame puts no face on the hands and does find the groom. A guard against weak detections was
written for that path and removed again - its threshold had been set on Haar confidences, a scale
the YuNet path does not carry. What remains is the Haar path: the Portrait studio applies its face
sliders to the hands and not to the groom there, and the Faces tab is how a photographer excludes a
face today. Moving the parse onto YuNet's faces is the fix; it changes what the renderer derives at
render time, so it is its own decision rather than part of this one.

## 9. HSL bands centred on the colours they are named for

The eight bands were spaced evenly in the working space's hue (linear Rec.2020), which is not where
the named colours are: photographic green fell under the Yellow slider, yellow under Orange and
magenta under Purple. The rest of the product already assumed Lightroom-style centres in sRGB
(`state.rs::band_of_hue`, `aura_curate::bw::BAND_SPLIT`). The renderer's centres are now those sRGB
hues - 0, 30, 60, 120, 180, 240, 280, 320 degrees - carried into the working space
(`tonemap::HSL_CENTRES`), and each band reaches exactly to its neighbours' centres, so the weights at
any hue still sum to one. A test converts each named colour and requires its own band to win.

This changes how an existing recipe with HSL edits renders, so the engine string moves to `hsl.3`.
It is the right way round: a Green -50 made by eye on the old bands was correcting the wrong colour,
and a style imported from Lightroom now lands on the colours it was made on.

## 10. Smaller

- Camera bodies are named once ("Canon EOS 5D Mark IV", not "Canon Canon ..."), in the integrity
  pass's report. The ingest clustering key is unchanged, so no camera is re-identified.
- The lens switches say when there is no profile or measured correction to apply.
- Auto edit says "Nothing changed: AURA judged this photograph already balanced" when it decided so.
- The Autopilot panel says, before a run, that while nothing is calibrated every deciding step waits
  for review (`calibrationVer` 0 in the ledger).
- `scripts/test-everything-gui.py` keeps its licence state in the run's folder (`AURA_LICENCE_DIR`),
  so export is testable and the machine's licence file is never touched; attaches ONNX Runtime and
  the models when present; and takes real portraits from `AURA_TEST_PORTRAITS`.

## 11. Not defects, after reading the code

- **Body definition** (in the Hair & attire tab's "Hair and body" group) shapes arms and collarbones;
  acting away from the head is what it is for.
- **Hair edge** acts only where hair meets background; a veil between them leaves nothing to calm.
- **Catchlight** takes its colour from the chosen light colour; the brown disc in the test was the
  test's colour input.
- **The structural recommendation** - feed the working portrait detector into People - would not
  bring People to life on its own: grouping needs the face recogniser and Emotion its head, and both
  are untrained placeholders too.

## 12. Not done, and why

Model training, GPU rendering, generative fill, measured camera and lens profiles, and HDR or
panorama merge need data, hardware or targets this machine does not have. Colourise and solid
background colour still act on whatever is inside the ellipse; both are meant to be used with an AI
selection, and protecting skin by colour alone would also protect brown hair.
