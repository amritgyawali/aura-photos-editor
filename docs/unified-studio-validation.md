# Unified studio validation - 2026-10-06

What was run on the reference Windows laptop (8 GB, four cores) for ADR-0088, and what it
showed. Every run below is the real desktop window, started with an isolated catalog by
`scripts/test-wedding-run.py`, with a genuine mouse click on **Finish a whole folder**. All
pixel work is AURA's; the script only reads the files AURA wrote.

## Ten photographs with known problems

Real photographs spoiled on purpose (exposure, colour cast, blur, a burst), scored against the
unspoiled originals. Errors are the distance of the frame's mean CIELAB from the original's.

| | Result |
|---|---|
| Delivered and verified | 6 of 6 selected, 0 failed edits, 33 s |
| Out-of-focus frame | left out |
| Motion-blurred frame | left out |
| Burst of three | sharpest delivered, two left out |
| Good frames left out | 0 of 5 |
| Median lightness error, wrongly exposed frames (4) | 14.2 before, 11.9 with the previous engine, **3.8** now |
| Median colour error, frames with a cast (4) | 10.2 before, 8.6 with the previous engine, **6.6** now |

Per frame: three portraits 0.6 to 1.0 EV under were lifted to within 0.3 to 4.2 of the
original's lightness. A tungsten portrait's colour error went from 13.7 to 6.7 and a
fluorescent one from 6.6 to 2.8. Not corrected: a fluorescent cast where the two estimates
disagreed, and a table scene 0.9 EV over under warm light with no people in it. Made worse: a
correct portrait in a pale pink blouse on a pink backdrop was read as a cast and cooled (colour
error 0.1 to 8.1) - the limit ADR-0088 records.

## 62 finished photographs, cull off

62 delivered, 62 verified, 0 failed edits. 3.3 s a photograph (2.2 s edit and retouch, 1.0 s
export at 1620 px). Peak committed memory 658 MB; sampled every ten seconds it stayed between
220 and 630 MB with no growth from photograph to photograph. 34 faces found, 278 retouch
operations.

Half moved by less than 1.7 from the original. **8 of 62 moved by more than 5**: frames whose
whites sit low on purpose were lifted, and frames dominated by one colour were read as a cast.

## Eight camera-size photographs

9.4 to 24 megapixels, 16.3 on average: 8 delivered at full size, 8 verified. 13.2 s a
photograph (2.5 s edit and retouch, 9.6 s render, encode and read back). Peak committed memory
1.24 GB.

## Two thousand photographs

**Not run.** The collection built for it was written to drive D:, which Windows marks "Full
Repair Needed", and the drive emptied the folder at frame 1,144. The photographer then asked
for a ten-photograph test. What the measurements above say about a run of that size: at camera
size it is a little over seven hours on this laptop for 2,000 delivered frames, fewer after
the cull; memory did not grow across 62 photographs; the edit report is written a photograph
at a time; and one failed photograph does not stop the rest. The run itself is unproven.

## Automated checks

- `cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.
- `aura-app`: 163 tests pass, 6 ignored (they need a folder of photographs). `aura-recipe`,
  `aura-render`, `aura-vision`: 564 pass, 4 ignored.
- UI: 77 files, 681 tests pass; type-check and production build pass.
- `check-banned` clean; IPC surface 301 defined = 301 registered = 301 invoked; 83 contracts locked.

## Found along the way

- The first sixty-photograph run died with "memory allocation of 12582912 bytes failed": drive
  C: had under 1 GB free, so the page file could not grow. Three stale cargo target folders
  were removed with the photographer's approval (15 GB).
- The learned analysis pass took 175 s on sixty photographs and its result could never be read
  as complete. It is now skipped by default (ADR-0088).
- In the timeline a frame from another camera sat inside a burst, and a neighbours-only rule
  delivered two frames of three. Bursts are now found across a window of frames.

## Not checked here

Camera RAW files; a run with a look or reference applied; retouch quality (the operations ran
and were exported, but were not scored); black and blown frames in the app (unit tests only).
