# AURA-ML-5025 - A chapter edit was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, in the timeline, with the chapter strip unchanged. The refusal is total: nothing was written.

## What actually happened

One of the four editing entry points on `StoryService` refused. Each has its own rules and all of them refuse rather than repair, for the reason phase 06 gives for `AURA-SEC-9005`: a "helpful" partial application of an edit the photographer did not ask for is worse than a message.

**`set_chapter`** - unknown segment id.

**`move_boundary`**
- the segment is the last one, so there is no boundary after it;
- the new time is outside `[start_ts of this segment, end_ts of the next]`;
- the move would leave either side with zero frames. A chapter with no photographs is not a chapter.

**`split_segment`**
- the photograph is not in this segment;
- the photograph is the first one, so one side would be empty.

**`merge_segments`**
- the two segments belong to different projects. Refused, not fixed - the same class of boundary `AURA-SEC-9004` guards for people;
- they are not adjacent. Merging chapters 2 and 5 would silently absorb 3 and 4, and no dialog asked about those.

## What AURA does automatically

Nothing is written and no journal entry is recorded. The panel re-reads the outline and redraws from the unchanged truth, so a refused edit cannot leave the screen and the catalog disagreeing.

## Operator steps

1. The detail names which rule fired. "not adjacent" and "would empty a side" are the two that reach support, and both are usually a stale strip: the UI was drawn before a re-analysis moved the boundaries.
2. Reload the timeline and retry. `story_outline` is cheap - it is one indexed read of `v_chapter_summary`.
3. A repeated refusal on the *same* ids after a reload means the segment rows and the `segment_images` rows disagree. `SELECT COUNT(*) FROM segment_images WHERE segment_id = ?` against `segments.image_count` finds it; a re-segmentation rebuilds both together.
4. Locked chapters are **not** a cause of this code. A `user_locked` chapter refuses *automation*, never the photographer.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The people equivalent: `docs/runbooks/AURA-SEC-9005.md`
- `docs/adr/ADR-0016-story-ipc-surface.md`
