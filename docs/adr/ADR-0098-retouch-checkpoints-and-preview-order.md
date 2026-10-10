# ADR-0098: Retouch checkpoints, and the order previews are asked for

Status: accepted
Date: 2026-10-07

## Problem

Speed is the first thing a photographer comparing AURA with Lightroom, Retouch4me or Imagen
notices. After ADR-0097, a retouched 6 MP portrait took 4.6 s for the quick look and 16.5 s at
full quality on the reference laptop - and every brush stroke, every Apply and every before
view paid that again, because each render ran every stage and every retouch operation from the
start. Almost all of that time is the retouch stack, and almost every render repeats the start
of the one before it: a stroke is one operation on top of the saved stack, Apply turns that
draft into the saved operation, and the before view is the stack left off.

The window also asked for the quick look and the full-quality preview at the same time, so the
two shared the processor and the quick look arrived later than it needed to - and a slider
dragged through several versions rendered a full-quality picture of each.

## Decision

### Checkpoints around the retouch stack (`aura-render/src/retouch_cache.rs`)

The engine keeps, per photograph and level, the working buffer **just before the stack** and
**just after it**, in a shared, bounded store (1 GiB, 16 entries, least recently used out) that
the application hands every engine it builds. Keys:

- before the stack: the photograph, the level, the purpose and the canonical hash of the recipe
  with the stack, its mattes, sharpening, effects and geometry left out (those come after);
- after the first *k* operations: that key, the mattes, and each operation in order with its id
  left out - an id names an operation and changes no pixel, which is what lets a draft and the
  operation it becomes on Apply share a checkpoint.

A render starts from the longest stored prefix and runs only the operations after it, then the
stages after the stack. Resuming reads mattes and texture references from the before-the-stack
buffer, exactly as a whole render does, so **the result is the same pixel for pixel**; a
texture restore that measures the skin acne clear left, with the acne clear already inside the
checkpoint, starts from before the stack instead. `tests/retouch_checkpoints.rs` renders seven
stacks in sequence through a cached and an uncached engine at two levels and requires
identical bytes. Tiled renders of very large frames and every render without a photograph id
(the golden suite, the parity harness) do not use checkpoints.

### Order of the window's requests (`ui/src/state/previewCache.ts`)

The full-quality preview is asked for when the quick look has arrived, not beside it, and only
if that version is still the one on screen. Dragging a slider through five versions renders
five quick looks and one full-quality picture.

### Live drafts at full quality once the brush rests (`useRetouchDraftPreview.ts`)

A draft is still rendered quick first. When the brush has rested for 1.2 s the same draft is
rendered at full quality - with checkpoints, that is one operation - and a new stroke cancels
it.

### The rest of the stack, on every core

Profiling the whole 32-operation stack at full resolution showed that a third of it was not the
operations at all: painting each operation's brush selection (13 strokes a quarter of the frame
wide, for every face operation), multiplying it by its matte, and rendering eight segmentation
mattes with their edge refinement. Strokes are now painted row by row in parallel, selections and
mattes are computed in parallel, the eight mattes render at the same time, and an operation whose
selection is identical to an earlier one in the same render - the face's evening, light and
smoothing share one - reuses it instead of painting it again. A selection limited by brightness
reads the pixels as they are at that point and is never shared. All of it is the same pixels
as before; every renderer test, the golden suite and the checkpoint test pass unchanged.

### What a release build adds

A release build of the same code (opt-level 3) was measured at about 10 % faster than the
development build the launcher makes, because the pixel crates are already optimised there. The
speed is in the algorithms, not the compiler flags.

## Measured

`crates/aura-app/tests/full_quality_preview.rs` after Auto advanced retouch, development build,
8 threads:

| Retouched acne portrait, 2048 x 3072, 32 operations | ADR-0097 | Now |
|---|---:|---:|
| Quick look, first time | 4.6 s | 3.3 s |
| Full quality, first time | 16.5 s | 12.5 s |
| Brush stroke on top, quick look | 4.6 s | 0.7 s |
| Brush stroke on top, full quality | 16.5 s | 1.9 s |
| Before retouch, full quality | 1.1 s | 0.9 s |
| Any view seen before | 0.06 s | 0.06 s |

| 28 MP camera JPEG, 6554 x 4369 | ADR-0097 | Now |
|---|---:|---:|
| Quick look, first time | 3.8 s | 1.8 s |
| Full quality, first time | 59.4 s | 31.1 s |
| Brush stroke on top, quick look | 3.8 s | 1.1 s |

The whole retouch stack on the portrait, measured alone (`tests/retouch_profile.rs`), went from
15.7 s to 10.9 s; acne clear is now 6 s of it, and the next place to look.

## Consequences

- Retouching by hand - strokes, Apply, Undo of the last operation, before and after - re-renders
  only what changed.
- A change to a global setting (exposure, colour, a profile) still re-renders the whole stack,
  because the stack runs after those stages. Acne clear (half of that render) and a GPU
  backend are what remain between this and a render in about a second.
- Memory: up to 1 GiB of checkpoints, on top of ADR-0097's caches; all are bounded and
  **Clear cache** empties them.
