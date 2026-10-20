# ADR-0125: Colour fringing measured from the photograph, and a defringe that keeps purple things purple

Status: accepted. Date: 2026-10-17.

A professional's first pass over a photograph, before any retouching, is the lens: Lightroom's
"Remove Chromatic Aberration" checkbox and its Defringe sliders. AURA had neither. The renderer
could apply a lateral chromatic aberration correction (phase 23, a radial scale per channel), but
only a measured lens profile could fill it, so with no profile Auto advanced retouch reported
"chromatic aberration left as shot rather than guessed". And there was no operator at all for the
purple and green rims round bright edges. This adds both, each measured from the photograph and
each refusing to act where the measurement could be wrong.

## 1. Lateral chromatic aberration, measured

`crates/aura-app/src/lateral_ca.rs`. A lens images red, green and blue at slightly different sizes,
so away from the centre the three images of the same edge land a fraction of a pixel apart. That is
a radial magnification of one channel against another - the whole of the renderer's model - and the
photograph shows it:

- **Read.** Strong green edges that cross the radius, at their own centre (no stronger response one
  pixel in or out), away from the middle third. Each is read across, a quarter pixel at a time, in
  all three channels; a channel's edge is where it crosses half way between its own two sides. Only
  clean, single, roughly colourless steps count: every channel steps the same way by a comparable
  share of its own level, so the edge of a red dress on a green lawn says nothing.
- **Fit.** Each edge gives a shift at a radius; the scale is the least-squares slope through the
  origin after a robust trim. A half-height crossing does not move when a channel is merely softer,
  so axial aberration and a JPEG's soft chroma do not register; a channel shifted the same way
  everywhere (chroma siting) makes opposite sides of the frame disagree.
- **Act only on what is visible, consistent and confirmed.** A channel is corrected when its shift
  at the frame's corner is a third of a pixel or more, three standard errors from zero, and the same
  sign in three quarters of the eight directions round the frame. 85 % of it is corrected - an
  under-correction leaves a fainter fringe of the same colour, an over-correction paints one of the
  opposite colour. Then the correction is rendered through the renderer's own resampler and measured
  again; if more than 60 % of a corrected channel's shift is left, nothing is changed.

Phase 23 withheld fringing on its estimated lens route because a correction fitted from edges it
was not measuring can invent the opposite fringe. This route measures the misregistration itself and
confirms the result on the corrected pixels, which is why it may act where that one may not.

On a synthetic scene of 11,504 usable edges, a lens painted at red x1.0015 and blue x0.9988 (1.08 px
and -0.87 px at the corner) was measured at +1.07 px and -0.86 px, corrected, and measured again at
+0.17 px and -0.14 px - the 15 % left on purpose. A clean frame reads 0.00 px; a lens at x1.0002
reads 0.14 px against 0.144 painted and is left alone as invisible. Red discs on green, a channel
shifted 0.6 px one way everywhere, and a blue channel blurred twice as much as green are all left
alone.

**The measurement space is the correction space.** The renderer corrects in linear Rec.2020, where a
working-space red is 0.63 sRGB reds plus 0.33 sRGB greens. The same shift read on sRGB pixels is not
the same number: the end-to-end test first measured its rendered output in sRGB and read a 0.16 px
working-space residual as 0.34 px, because an sRGB red is 1.66 working reds less 0.59 greens. The
planner measures on `CatalogFrames`' proxy, which is already the renderer's working frame, and only
when the frame says so (`InputKind::Working`).

`GlobalPlan::fringing` carries the decision, so Auto enhance writes it in its "Light & colour" step
and Auto advanced retouch in "Lens & perspective" - one answer whichever button asked.
`smart_edit::apply_fringing` writes `lens.ca` and the two scales only where no lens profile or person
has set chromatic scales, keeps distortion coefficients already there beside them, and writes nothing
the second time.

## 2. The correction left a line round the frame

`aura_render::geometry::correct_ca` sampled a channel scaled up beyond the frame's border and read
black there - correct for distortion and keystone, which scale so that nothing samples outside and
black means a hole, and wrong here: one channel missing along the border is a red or blue line round
the photograph. It reads the nearest edge pixel now, and shares its rows across cores with the same
per-pixel arithmetic (`a_ca_correction_inside_the_frame_is_the_one_core_arithmetic` compares bits).

## 3. Defringe

`Tool::Defringe`, `crates/aura-render/src/retouch_defringe.rs`. Axial aberration and sensor bloom
leave a coloured rim on the dark side of a bright edge - purple in front of focus, green behind it. A
raw converter removes it by colour, with the risk that a purple dress, a magenta flower and the glow
of a purple uplight - every Indian wedding reception has them - are purple too. A pixel is a rim
when all four hold:

| Test | Measured as |
|---|---|
| Beside a bright area | A fourth-power mean of the neighbourhood's light, two to five times the pixel's own |
| In the fringe's colour | Hexcone hue in the working space, 255-325° (green 80-150°), chroma above 0.06 |
| Not the colour of its calm surroundings | The share of that colour among the calm (non-edge) pixels around it, below 0.15 to 0.40 |
| Not beside a coloured light | The colour of the bright neighbourhood itself is not in the band |

and it moves to **the colour of its calm, unfringed surroundings at its own luminance**, never to
grey by default - so a pixel that was selected but is really the object's colour moves almost
nowhere. Where a rim has no calm side of its own (a twig three pixels wide against a sky) it goes
grey rather than taking the sky's blue.

**The first version greyed a purple wall's edge.** Read evenly, the calm surroundings of a pixel at
the wall's edge are mostly the bright window beside it - calm, neutral and much larger - so the wall
read as mostly not purple, below the protection, and its edge was changed by up to 0.041. A rim is the dark
side of its edge by definition, so every calm pixel now counts by how dark it is (an inverse square
of its light), and the wall's own interior decides. `a_purple_object_beside_a_bright_window_keeps_its_colour`
fails on the even weighting and passes on this one.

**It was ten times too slow.** Read at full resolution, a defringe over a 24-megapixel export blurred
a dozen full-size planes: 11.4 s. Every reading except the pixel's own colour and light is a mean
over the reach or more, so they are taken on cells a third of the reach across and read back
bilinearly, and only the rim pixels are kept, as a sparse list: 1.1 s.
`on_a_coarse_grid_rims_go_and_purple_things_stay` holds the coarse grid to the same three decisions.

`warmth` is the purple strength and `tint` the green one, `radius` the reach, `amount` both. It is in
the Retouch tool list under Cleanup, with purple on and green off by default.

**Automatic use is purple only.** `smart_edit` measures with `retouch_tools::defringe_rims` - exactly
the decision rendering makes - and proposes `auto-scene-v1-defringe` over the whole photograph when 40
or more rim pixels per million (and at least 80) would visibly change. Green is left to a person:
the edge of every leaf against a bright sky is green as well. In Auto enhance it is its own scene
step ("Purple fringes"); in Auto advanced retouch it is cleaned off with the dust in step 3.

## 4. The engine string is not bumped

Neither change moves a pixel of an existing recipe except one: a recipe that already carried
chromatic scales above one (from a phase 23 lens profile) now renders its outermost ring of pixels
with the channel's edge value instead of black. That is a defect removed, and ADR-0123 and ADR-0124
set the precedent of stating such a change rather than invalidating every render. Defringe is a new
tool and no stored recipe contains it.

## What is not claimed

Every number here is from synthetic frames whose aberration and fringes were painted, read back
through the real measurement, renderer and resampler: the container this was written in reaches no
photo source. Real lateral aberration is not always a pure radial scale (decentred elements, a
sensor not square to the lens), and the sector agreement test is what refuses those rather than a
model of them. A camera JPEG has usually had its lateral aberration corrected in camera, and the
measurement should then find it in register. Defringe's hue bands are set in the working space from
the hues purple fringing has in sRGB, not from a study; rims wider than the reach, or on the bright
side of an edge, are not found; and nobody has yet looked at its result on a real backlit veil at
100 %.
