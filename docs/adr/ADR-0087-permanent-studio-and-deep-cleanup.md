# ADR-0084: Permanent reference studio and texture-preserving cleanup

Status: accepted
Date: 2026-10-05

## Context

The photographer requested the earlier plum studio screenshots as the only default desktop interface. Main had replaced that shell with workflow stages and an OS-dependent theme. Earlier local retouch work was also absent from main. The original checkout has damaged Git objects, so integration uses a fresh clone of main and preserves the original working files.

## Decision

Restore the six-section sidebar (Start, Photos, Auto edit, Instagram style, Export, Advanced), collection list, photo-first Develop workspace, and fixed plum/lavender palette. Remove alternate onboarding, theme switching, and stage navigation. Old local theme values cannot select another shell. Keep newer batch selection and per-photo saved retouch preferences. Advanced tools remain accessible inside the same shell and mount only when expanded.

Restore opt-in Deep acne cleanup, with explicit dark-mark removal and up to 220 measured spot repairs per face subject to the existing recipe operation budget. Segmenter masks and landmark exclusions protect facial features. Multiscale spot detection chooses nearby clean donors; small donors reflect texture without enlarging pores. The finishing frequency-separation pass separates mid-scale unevenness from fine texture and uses skin-weighted filtering to avoid dark edge contamination. Existing recipes default to the original two-band behavior. Automatic reruns remain replaceable, originals remain untouched, and manual edits are retained.

Normalize quarter-turn portraits before skin segmentation and transform saved mattes back to source coordinates. This improves rotated photos without claiming complete skin coverage.

## Limits

These deterministic methods cannot guarantee every blemish is removed on every photo. Dark-mark removal can also affect intentional marks and should be reviewed. Borrowed donor texture is an approximation, not recovered original detail. Mixed-orientation groups, occlusion and low-resolution faces still need manual correction.

## Validation

See `docs/studio-restoration-validation.md` for actual test and desktop results. No merge is performed: the requested delivery is a branch and pull request targeting main.
