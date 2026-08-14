# AURA-ML-5029 - A split, merge, lock or keep-hint change was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and the grouping exactly as it was. Nothing partial happened: every edit is one transaction, and a refusal is a refusal before it starts rather than a rollback after it fails.

## What actually happened

One of six rules refused, and the detail line names which. In the order they are most often hit:

1. **"splitting before the first frame would leave the original moment empty"** - the split point is the frame that *starts the new half*, which is the same convention `StoryService::split_segment` uses for a chapter. Splitting at the first frame asks for a moment of nothing.
2. **"that photograph is not in this moment"** - almost always a stale UI: the grouping was re-run between the panel opening and the button being pressed, and the frame moved to another stack.
3. **"those two moments are in different weddings or different chapters"** - moments merge inside a chapter. Two moments in different chapters are two events by phase 07's reckoning, and merging them would make a moment that no chapter contains. Cross-*project* is refused harder and for a different reason: nothing in this product crosses a wedding.
4. **"a moment cannot be merged with itself"**.
5. **"that photograph is not in any duplicate set of this moment"** - "keep this one" moves a hint inside a set. A frame that is not in a set has nothing to be kept over.
6. **"there is no grouping edit to undo in this wedding"**.

## What AURA does automatically

Nothing, and that is the correct behaviour. `Recovery::AskUser`: every one of these is a statement about what the photographer asked for, and repeating it changes nothing. There is no retry, no partial application and no "did you mean".

## Operator steps

1. Read the detail line. It names the rule, not just the failure.
2. For cause 2, refresh the moments view and try again. The moment ids in the message can be looked up directly: `SELECT * FROM moments WHERE id = ?`.
3. For cause 3, `SELECT id, project_id, segment_id FROM moments WHERE id IN (?, ?)` shows which half of the rule was broken.

## The one that is not recoverable, and what to do instead

**A merge cannot be undone by splitting.** `MomentService::undo` unlocks a merged moment rather than reconstructing the two it came from, and the reason is in the data: the absorbed moment's id is gone, and re-deriving the boundary would be a guess dressed as a restoration. What the unlock does is hand the frames back to the next grouping pass, which reconsiders them from scratch.

If the original split mattered, the recovery is to split by hand at the frame where the two moments met - `moment_edits` records the merge with both ids and the size, so the shape of what was there is recoverable from the journal even though the rows are not.

## When this is not the problem

A photographer who cannot find the split control on a single-frame moment is not hitting this error - the control is disabled, because `canSplitAt` in `MomentStack.tsx` mirrors rule 1 in the interface. A rule the photographer can see is better than one they discover.

## Related

* `AURA-SEC-9005` - the same shape for a people edit.
* `AURA-ML-5025` - the same shape for a chapter edit.
