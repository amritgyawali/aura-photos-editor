# ADR-0185: Portrait studio faces agreed between the two face detectors

Status: accepted, 2026-10-28. Follows ADR-0184 section 8.

## Context

AURA has two face detectors. The renderer's portrait parse (`aura-portrait`, Haar cascades)
re-derives faces from the pixels at render time, and every Portrait studio tool, the regions view and
the face picker act on what it finds. Automatic retouch uses the learned detector (YuNet,
`aura-vision::portrait`). On 989A3150 the parse took a standing woman's clasped hands for a face and
missed a groom in sunglasses; the learned detector did neither. So a face slider moved on that
photograph slimmed the hands and skipped the groom.

The renderer cannot call the learned detector: `aura-vision` sits above `aura-render`, and moving the
detector below the renderer is an architectural change across three crates. The photographer chose
the smaller route: agree the two when the finish is saved.

## Decision

The first time a Portrait studio save needs the parse on a photograph (`StudioFinish::needs_parse`)
and the photographer has not chosen faces, `studio_faces::reconcile` compares the parse's faces with
the learned detector's on the same proxy, and writes the answer in two forms the renderer already
reads:

- each face only the learned detector found becomes a face hint (`studio-faces-hint-N`), which the
  parse accepts as it accepts a face a photographer drew;
- the finish's chosen faces become the faces both accept plus the added ones, so a parse face the
  learned detector rejects is left out of every face tool.

The panel shows a note saying what was added and left out, and the Faces tab shows the choice.

Three rules keep it from doing harm:

1. **A small face is never evidence against itself.** The learned detector deliberately skips small
   faces in a crowd, so a parse face is left out only when it is at least 0.6 of the width of the
   smallest face the learned detector did find.
2. **When the two agree, nothing is written.** The Faces tab still says every face, and the recipe is
   as before.
3. **Once only.** `StudioFinish::faces_reconciled` is set after the first comparison and carried
   forward on every save, so a photographer who then presses "Use every face" keeps that choice; a
   decision a person made is never redone.

A comparison that cannot run - no proxy yet, a detector error - never refuses the save; it is logged
and tried on the next save. `faces_reconciled` is serialised only when true, so every existing recipe
is byte-identical and keeps its hash.

## Consequences

- On 989A3150 the face tools act on five faces: the bride, the groom, two standing women and the man
  cut off at the top - and not on the hands.
- The comparison costs one learned-detector pass the first time a parse-dependent tool is used on a
  photograph (about a second on the 2048 px proxy); later saves skip it.
- The regions view and the face picker now list the added faces, because both read the recipe's
  hints. The picker still lists the left-out hands - unticked - because the parse still finds them.
- The real fix remains the architectural one: one detector for every view. This decision is
  deliberately reversible: deleting the `studio-faces-hint-` masks and clearing the chosen faces
  returns a recipe to the parse alone.
