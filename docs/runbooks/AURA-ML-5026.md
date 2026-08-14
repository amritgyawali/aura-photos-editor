# AURA-ML-5026 - The timeline could not be split into a plausible number of chapters

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence above a timeline that still has chapters in it - one per clear break in the day - and an invitation to adjust the boundaries.

## What actually happened

Section 10.1 requires that no wedding produce fewer than 6 or more than 20 chapters, and section 6.2 achieves it by **searching** the PELT penalty rather than fixing it: a penalty tuned on a ten-hour wedding gives two chapters for a registry office and forty for a three-day Nepali wedding.

This code fires when the search finished without landing inside `changepoint::CHAPTER_BAND`. Four ordinary causes:

1. **The wedding is genuinely short.** A 90-minute elopement has four moments in it. The band is a statement about a full wedding day and this is not one.
2. **Coverage is low.** Scenes exist for 30 % of the frames, the signal between consecutive labelled frames is mostly noise, and the change-point detector has little to detect. `StoryOutline::coverage` is the first thing to read.
3. **Every frame has the same posterior**, which is what a placeholder classifier does - see condition C1 in the phase 07 exit report. With a flat signal the only boundaries are the hard time gaps, and there may be fewer than six of them.
4. **Timeline times are missing or wrong.** Segmentation is over `photo.timeline_time`; a card whose camera clock was never aligned lands its frames in 1970 and the day becomes two clusters fifty years apart. Phase 01's clock alignment is what prevents it, and `AURA-IO-1009` is what reports it.

## What AURA does automatically

Falls back to **gap-only segmentation**: every time gap above `changepoint::HARD_GAP_MS` (20 minutes) becomes a boundary and nothing else does. That is the segmentation photographers already understand - "these are the times I moved location" - it is deterministic, it never over-segments, and it is honest about being a fallback. Every segment it produces carries the reason string `"chapter boundaries came from time gaps only"`, and confidence is capped so the chapters are flagged for review.

The pass completes and the wedding is usable. Invariant 9: a typed error, a fallback path, and a telemetry event.

## Operator steps

1. Read `StoryOutline::coverage` before anything else. Below about 0.6 this code is a coverage report, not a segmentation bug.
2. Read the logged `penalty_low` / `penalty_high` bounds and the chapter counts they produced. Two counts that jump from 3 to 27 across one penalty step mean the signal has one dominant break and no structure - cause 3.
3. `SELECT MIN(timeline_time), MAX(timeline_time) FROM photo WHERE project_id = ?`. A span of decades is cause 4, and the fix is phase 01's camera clock offset, not this phase.
4. The photographer's own boundaries survive everything. Chapters they have split, merged, renamed or moved are `user_locked` and the next re-analysis is built around them rather than over them.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Camera clock alignment: `docs/runbooks/ingest.md`
- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` section 6
