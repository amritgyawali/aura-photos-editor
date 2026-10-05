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

### A run that fits in memory

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

## Verification

`crates/aura-app/src/measured_cull.rs` unit tests (focus, motion, exposure, bursts, the
hand-edited and unmeasured guarantees, the withdrawal), `crates/aura-app/tests/one_click.rs`
(the run writes `photo-cull.json`, keeps a hand-edited frame and reports the cull),
`ui/src/components/workflow/FinishFolder.test.tsx`, and `scripts/test-wedding-run.py`, which
presses the real button in the real window and scores the cull and the edit against a
collection with known ground truth.
