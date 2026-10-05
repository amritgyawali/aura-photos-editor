# ADR-0088: One studio, and a wedding finished from one folder

Status: Accepted
Date: 2026-10-05

## Context

Two versions of the product had drifted apart on the photographer's machine, and the desktop
carried a shortcut for each ("AURA Photo Editor" and "AURA Photo Studio") that both opened
the same build:

- `main` had the permanent plum studio (ADR-0087) with deep blemish cleanup, but none of the
  adaptive work.
- `feat/expert-adaptive-editing`, never pushed, had per-face adaptive retouch, scene intent
  and exposure judged by the people in the frame (ADR-0086), in the older stage-based shell.

Inside the studio there were also two different automatic edits. **Auto edit all photos**
edited each photograph with the measured edit (`smart_edit`: scene, people-aware exposure,
two-estimate white balance, noise, sharpening, then retouch). The native **Complete
collection workflow** - the only path that also exports, and the only one that survives a
tab change - used the cloud task's local fallback instead: a thumbnail histogram, with none
of that. It was also hidden under Advanced, it ignored the look chosen on the start screen,
and its cull step did nothing: phase 12's cull fuses four learned sub-scores, every one of
them still from a placeholder head, so the run kept every frame and said so. It held every
recipe of the run in memory to write one report at the end.

A photographer's request is one sentence: choose the wedding folder, and have it imported,
culled, edited, retouched and exported, each photograph on its own terms, for two thousand
photographs or more.

## Decision

### One product

`feat/expert-adaptive-editing` is merged into `main`. The studio shell is the only
interface; the engine is the adaptive one. The shortcut is installed once, as **AURA Photo
Studio**, and the launcher removes the older name.

### One per-photo edit

`one_click_commands::edit_one` is the per-photo edit of an unattended run, and it takes the
same three paths the studio's own batch takes:

1. **A look was chosen** - the edit profile over the photograph's measured correction, then
   portrait retouch, then the reference fitted on top when there is one.
2. **A provider is configured and no look was chosen** - the provider's grade, then portrait
   retouch. Unchanged.
3. **Otherwise** - `enhance_photo`, the measured edit and retouch in one pass.

The thumbnail-histogram fallback is no longer an edit anybody receives. A photograph edited
by pressing **Auto edit** in the studio and the same photograph finished unattended now get
the same recipe.

### The look travels with the run

`AutomaticStartInput` and `OneClickFinishInput` gain `look` (`AutomaticLookInput`: profile id
and strength, reference id and strength, as whole percentages so the input stays `Eq`) and
`keep_everything`; `AutomaticStartInput` also gains an optional `destination`. All are
`#[serde(default)]`, so an older caller's payload still parses. This is a change to a frozen
contract; `contracts.lock` is re-locked with this ADR.

Blast radius, from `grep -rn "OneClickFinishInput {\|AutomaticStartInput {\|automaticStart\|oneClickFinish("`:
`one_click_commands::automatic_start` (builds the finish input), `crates/aura-app/tests/one_click.rs`
(three literals, updated), `ui/src/ipc/client.ts` (`automaticStart` now takes the typed input),
`ui/src/components/workflow/FinishFolder.tsx` (new caller) and
`ui/src/components/workflow/OneClickRunner.tsx` (unchanged: it omits the optional fields). The
shell's two handlers in `ui/src-tauri/src/main.rs` pass the input through and are unchanged.

### A cull that is measured (`measured_cull.rs`)

When the learned cull cannot run, the frames a photographer rejects on technical grounds
alone are found from pixels, at one analysis scale (1024 px long edge) whatever the camera:

| Rule | Measurement | Threshold |
|---|---|---|
| Unusable exposure | brightest 1% of the frame; share of the frame at pure white | below 5%; above 60% |
| Out of focus | 99.9th percentile of gradient over the frame's own contrast, and the 99.99th | below 0.10, and below 0.16 |
| Motion blur | the weakest of four edge directions, and its ratio to the strongest | below 0.08, and below 0.5 |
| Burst duplicate | 64-bit difference hash between consecutive frames, and camera time | within 6 bits and 2.5 s; 3 bits when the camera recorded no time |

The focus and motion thresholds were calibrated on 34 real photographs against synthetic
Gaussian and motion blur of known size: no sharp original fell below either (softest 0.156;
direction ratios 0.74 to 0.88), a three-pixel Gaussian blur at the analysis scale measured
0.086, and a 25-pixel motion blur a ratio of 0.19 to 0.46. Within a burst the frame with the
sharpest faces is kept, and a frame whose eyes are clearly flatter than a sibling's loses to
it. One frame is kept from a burst of up to five, two up to twelve, three beyond.

It only ever errs toward keeping:

- A photograph a person edited by hand is never left out.
- A photograph that cannot be measured is delivered.
- If the focus and exposure rules would reject more than 40% of a collection they are
  describing a style, not mistakes, and are withdrawn for the whole run.
- Nothing is deleted. A frame left out stays in the collection, editable and exportable.
- Every decision, with its measurements and the frame delivered instead, is written to
  `photo-cull.json` beside the export, and summarised in the run notes.
- **Cull first** on the start screen switches it off.

### Bursts need not be neighbours

A burst is found by comparing each deliverable frame with the eight that follow it, not only
with the next one. The first real run showed why: a second camera's frame, or a rejected one,
often sits between two frames of the same burst in the timeline, and a neighbours-only rule
then delivered two of a burst of three. Frames the camera did not time must still be
neighbours.

### The learned analysis pass is skipped

The run used to start the autopilot's learned analysis (people, focus heads, moments, framing)
and wait for it. Every model that pass consults is a placeholder and nothing in this build is
calibrated, so its result may not be acted on; and the loop that waited for it could never
see it complete, because the progress row it polled exists only while the run does. On sixty
photographs it took 175 seconds and produced nothing the run used. It is now off unless
`AURA_LEARNED_ANALYSIS=1`, the run says so in its notes, and when it is switched on its
outcome is read from the run summary.

### Highlights as evidence of exposure

The histogram correction leaves a frame alone when its median sits in a wide normal band,
which is right for a finished photograph and wrong for one exposed a stop low: measured
against 62 professional photographs darkened by one stop, it left a median error of 0.69 EV.
`smart_edit::highlight_lift` adds a second witness. A frame whose very brightest tones (the
99.5th percentile) stop more than half a stop short of white, and that is not a night scene,
is lifted by 80% of that room, to at most one stop.

- With people in frame the lift replaces the "surroundings" cap, which answers a different
  argument, and is still stopped before any face would clip.
- Without people it takes three quarters of that, and no more than half a stop in a frame
  that is dark throughout.
- Skin at or near clipping (more than 12% of the face above 85% linear) is darkened by 0.2 to
  0.7 EV whatever the histogram says. It is a ceiling, never a target brightness for skin.

On the same 62 photographs one stop under, the prototype of this rule left 0.15 EV.

### A third witness for white balance

Two estimates (grey pixels, grey edges) had to agree before a cast was corrected, and on
finished photographs they agreed on a cast that was not there in 39 of 62: warm wood, foliage
and coloured clothes average to a colour. `white_patch` reads the brightest unclipped
near-neutral tones, which take the colour of the light and little else.

- When those tones are neutral, the light is, and nothing is corrected. In the prototype this
  removed 20 of the 39 false corrections and cost two real ones.
- When they show the same cast and at least a tenth of the frame is neutral area, 85% of a
  mild cast and 70% of a strong one is removed, instead of 65% and 50%.


`photo-edits.json` is written one entry at a time (`EditLog`) and closed even when the run
is stopped, so two thousand recipes are never held at once. The edit phase reports the file
it is on and an estimate of the time left.

### One press on the start screen

**Finish a whole folder** (`FinishFolder.tsx`) is the primary action of step three. It
chooses a folder and calls `automatic_start` with the look from steps one and two. The
native worker creates the collection, and the shell follows it through the existing
progress panel. **Choose photos** and **Choose a folder** remain for importing and editing
with a review before export.

## Consequences

- The measured cull judges technique, not content. It does not know which of two different,
  equally sharp photographs is the better picture, whether an expression is flattering, or
  whether a moment matters. It will keep a sharp photograph of nothing and a sharp frame of
  a blink that had no sibling.
- Deliberate motion blur without a sharp subject (a panning shot) is left out. Flash with a
  dragged shutter is not, because the subject is sharp.
- A burst is recognised only between consecutive frames. Two near-identical photographs
  taken with a different frame between them are both delivered.
- The thresholds are constants calibrated on a small set of web-sized photographs. They are
  conservative by construction and should be re-measured on real camera files.
- An unattended run is as long as its photographs: every frame is decoded, measured, edited,
  retouched and rendered on the processor. `scripts/test-wedding-run.py` measures it.
- A stopped run does not resume where it stopped; running it again edits every photograph
  again. A repeat edit saves nothing new, so the result is the same, but the time is spent.
- The highlight rule assumes a photograph should contain something near white. A finished
  photograph with a deliberate matte look, or a dim scene with no light source in frame, is
  lifted when it should not be: two of 62 finished photographs were, by 0.4 and 0.5 EV.
- White balance is still wrong where a scene offers no neutral reference. A portrait in a pale
  pink blouse against a pink backdrop is read as a white blouse in pink light and is made
  cooler; a strong cast with no people in frame is kept as the light's mood; and where the two
  estimates disagree nothing is corrected. The estimate itself is off by about 0.10 in
  log-chroma, half the size of a mild cast, which is why only part of any cast is removed.
- An overexposed frame with no people in it is not darkened: clipping without a face says
  nothing about intent. Its highlights are recovered and that is all.

## Verification

`crates/aura-app/src/measured_cull.rs` unit tests (focus, motion, exposure, bursts with a
frame between them, the hand-edited and unmeasured guarantees, the withdrawal), `crates/aura-app/tests/one_click.rs`
(the run writes `photo-cull.json`, keeps a hand-edited frame and reports the cull),
`ui/src/components/workflow/FinishFolder.test.tsx`, and `scripts/test-wedding-run.py`, which
presses the real button in the real window and scores the cull and the edit against a
collection with known ground truth.
