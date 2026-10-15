# ADR-0118: Stretch marks, moles, and repairs that respect structure

Status: Accepted
Date: 2026-10-13

ADR-0117 left six differences from the published results on the same six pairs.
Each was measured again, per face and per detail band, before anything was
changed. Two of the six statements were wrong and are corrected here; four were
defects or missing operations and are closed here. The measurements are in
[the paired validation](../evoto-paired-validation.md).

## 1. Stretch marks: evening that flows across a mark and stops at structure

A stretch mark is long, thin and faint, and an abdomen carries dozens. The spot
healers are built for compact marks and leave them. A smoothing wide enough to
clear them also flattens the hands resting on the abdomen and erases a navel,
which is what the multi-scale evening tried for ADR-0117 did.

Contrast does not separate the two: the darkest marks measured 0.28 across the
window and the outline of a finger 0.41. Shape does. A mark is a line with the
same skin on both sides, so its gradients cancel over a window wider than the
mark; the edge of a finger, a knuckle crease or the rim of a navel is a step, and
a step survives that window.

`Tool::StretchMarks` (`crates/aura-render/src/retouch_striae.rs`):

- **Structure is a step that survives, kept as a whole contour.** The mean
  log-luminance gradient over a window 0.18 of the evening radius, times the
  window: at or above 0.36 it is structure wherever it is found, and connected
  to such a cell, 0.16 is still the same contour. Hysteresis, because a contour
  kept only where it is strongest is a dotted line the evening leaks through.
- **A filter whose reach is cut by structure, not a blur with a mask.** A
  recursive edge-stopping filter (Gastal and Oliveira's domain transform) run on
  weighted colour, the weight, and the weighted cell coordinates together. A
  masked blur keeps a finger's edge and still averages the finger into the
  abdomen beside it; this one cannot cross.
- **The lean is taken back out here too.** Beside a contour the mean is read
  from one side. The coordinates that came through the filter say where, and
  `Lean::shading_between` removes the photograph's own shading between there and
  the cell, bounded to 15% of the tone and trusted only where the mean had
  support. Without it the edge of an abdomen comes out as a pale rim.
- **Grain is put back, limited to what clean skin carries.** The finest band,
  capped at 2.5 times its own median on the selected skin: the edge of a mark is
  larger than a pore.
- **A sample the evening would move a long way is not a mark.** Up to 0.75 in
  log luminance it is evened in full; at ln 3, a third or three times the skin
  around it, not at all. The cores of the deepest stretch marks measured sit
  between 0.45 and 0.75 and a mole at 1.3 or more. The first cut-off, 0.25,
  left those cores behind as dark slivers on clean skin.

It is planned for body skin only, after the spots are healed (section 3 says
why), and only when **Stretch marks** is above zero. No measure tried separates stretch-marked skin
from ordinary skin texture reliably (a newborn's cheek and a pale stretch mark
read the same in every band energy tried), so the operation does not decide for
itself whether a photograph has any. The **Body skin finish** preset turns it on.

## 2. An island of skin the segmenter calls an object

ADR-0117 let body skin grow into cells no label was sure of. On the maternity
pair that still left the lower half of the abdomen out: the segmenter is 87% sure
it is an object, and a sure label cannot be overruled by colour without also
letting a skin-coloured wall in.

Enclosure can. Unselected cells that cannot be reached from the frame's border
without crossing selected skin are surrounded by the body, and there "something
else" is not evidence of a thing. An island joins whole when four fifths of it is
the body's own measured colour and not claimed as clothing, hair or a face, so a
watch, a tattoo or a swimsuit stays out. Islands are looked for after the margin
growth of ADR-0117, because it is often that margin which closes one, and the
growth budget covers both.

## 3. A mole is dark in red, and has an edge

The presets remove dark marks, and removed moles with them. Redness cannot tell
them apart: a brown mole has a larger share of red than the skin around it,
exactly as an inflamed spot has. Brightness cannot either: an angry spot can be
half as bright as its skin.

The red channel separates pigment from inflammation. Blood absorbs green and blue
and passes red, so inflammation leaves red nearly where it was; melanin absorbs
everything and takes red down with the rest. It does not separate a mole from
the trace an old spot leaves on dark skin, which is pigment too and measured as
dark (0.31 of the skin's red against 0.11 and 0.27 for two moles). The finished
photographs removed the trace and kept the moles. What differs is the edge: the
trace fades into the skin over several frequency radii, and a mole has a step.

- **Rule:** a connected patch whose red is under half the red of the skin around
  it (a quarter of the distance between the eyes wide), with a step at its edge
  of at least the skin's red per frequency radius, is a mole. Neither it nor a
  mark touching it is healed. Read off their radial profiles, the two moles come to about 1.5 and 1.9 and
  the trace to about 0.6.
- **Each cell's own red is read as it is.** Averaged with its neighbours first,
  a mole three cells wide was outvoted by the skin beside it and healed, while
  the wide diffuse trace survived the averaging and was kept: the rule did the
  reverse of what it was for.
- **Where:** frequency healing (`Edit::mole_edge`, absent in older recipes) and
  the spot planner (`candidates`), which plans on the frame frequency healing
  leaves.
- **A face asks for the edge and a body does not.** `mole_edge` is the step the
  patch's border must have: 1 on a face, so the trace of an old spot is still
  healed, and 0 on body skin, which keeps every pigment-dark patch. A body has
  no landmarks to say what a dark patch is, and a navel is pigment-dark with no
  edge at all: with the whole abdomen selected, body spot healing rebuilt it as
  plain skin until this was a number rather than a switch. The hem of a garment
  in the soft edge of a body selection is kept the same way.
- **Setting:** `keepMoles`, on by default. Options saved before it existed keep
  moles from now on; that is a change of behaviour and is the intended one.

Body spots are healed before the stretch marks are evened. The spot search's
thresholds are multiples of what the skin does everywhere, so it is at its most
careful on skin still crossed by marks; the evening then clears what is left.

## 4. A repair is refused where the skin around it is not plain

Compared with the published results, the planned spot repairs changed about two
and a half times as many dark features as the finished photographs did. Three of
them were visible defects: a flat patch across a nasolabial fold, a smeared
corner of a mouth, and a blue-grey streak beside a nose where a colour-only
repair had taken the redness out of the fold itself.

Every check before this one reads a ring of sixteen points, and a fold running
through a repair meets that ring at two of them. `in_structure` reads every cell
between one and two and a half radii out and asks how far the slope there departs
from a single even slope: plain skin under any lighting has one, a crease has two
that oppose. When that spread, carried across the repair's own radius, exceeds
0.15 of the skin's brightness, the repair is refused. A compact colour-only
repair is exempt, because it keeps the light it finds; a long one is the redness
of the fold.

The planner's threshold is also never under a multiple of the skin's own robust
spread, the same multiples frequency healing uses.

With dark marks being removed, frequency healing may also work on the cheek
below an eye, the zone the confirmed spot repairs of ADR-0116 already reach:
freckles do not stop at the socket, and frequency healing rebuilds tone under a
compact mark and pastes nothing. Lids, the inner corner and everything above
the eye stay out.

## 5. Portrait skin finish is recalibrated per detail band

Measured inside face skin, as the share of each detail band a result keeps, the
published results keep 0.67 to 1.01 of every band on the adult faces. Portrait
skin finish kept 0.62 to 1.00, and less from 2 to 8 px on the Beauty faces in
particular (0.62 and 0.76 against 0.71 and 0.89). Attributed one control at a time, face smoothing
was most of it: it works on the band between pores and facial form, which the
published results leave almost alone.

The preset moves to smoothing 5%, light evenness 10%, micro dodge and burn off
and pore refinement 80%. From 4 px up it now keeps what the published results
keep within 0.02 on the adult faces. Nothing in the planner changed for this:
a preset is a set of control values, and the controls still mean what they
meant. Smooth portrait finish keeps its own heavier values and says in its
description that it removes more texture than the published results do.

## 6. Two statements in ADR-0117's validation that were wrong

- **"The Events pair is not pixel-aligned."** It is. An affine registration on
  local structure recovers a known test transform to a hundredth of a pixel and
  finds the published pair within 0.01 px everywhere. The large detail error
  that suggested otherwise is the edit: highlights recovered, shadows lifted,
  contrast lowered. `scripts/compare-reference-pairs.py` now measures
  registration for every pair instead of leaving it to be assumed.
- **"The published result is smoother overall."** Per detail band, inside face
  skin, it is gentler: over the three face pairs it keeps 0.67 to 1.01 of every
  band, where Portrait skin finish kept 0.46 to 1.00 and Smooth portrait finish
  0.35 to 0.98. It looks cleaner because it removes marks and softens lines
  while leaving texture, not because it smooths more.

Both came from reading a single number, or a picture, as a cause. A threshold on
a measurement is a statement about the instrument as well as about the world.

## 7. A matched look is kept and applied to other photographs

Choose finished reference fits one photograph to its own finished version.
With a name, the look it finds is kept as a personal look: what the finished
recipe adds beyond AURA's measured correction of that photograph
(`personal_style::look_of`). Applied to another photograph it starts from that
photograph's own correction and goes through the scene guards every profile
does. That is how one finished frame grades the rest of a shoot. On the
published Wedding pair the match ends 4.43 from the finished image and the kept
look, applied again through the preset path, 5.16 (13.54 untouched): the guards
hold its warmth to a fifth on a frame whose light is already warm.

It is carried by two optional fields on the existing command (`keepAs` in,
`keptLook` out) rather than a new one, so the shell's handler list and the
frozen IPC types are unchanged.

## What this does not establish

The stretch-mark, mole and structure rules and the preset's calibration were
set on six published photographs: one abdomen, ten faces. The thresholds are
measurements of those, not of weddings, and the same photographs were used to
set them and to check them. One dark mark on a cheek, spots between a newborn's
brows and the flush of its cheek, and under-eye lines still differ from the
published results. Exact equality with the
published results, complete removal and superior quality are not claimed.
