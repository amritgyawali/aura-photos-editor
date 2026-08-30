# AURA-ML-5124 - A photographer's choice of reference photograph could not be recorded

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Pinning or rejecting a reference frame did nothing. The part of the wedding is unchanged.

## What actually happened

One of three things, and all three are the panel and the catalog disagreeing about the tree rather
than anything a person did wrong:

1. **The node no longer exists.** The panel was showing a tree from before a re-pass, and the pass
   re-shaped it - a change point that was not there last time split a node in two, so the id the
   panel holds points at nothing.
2. **The photograph is not in that node.** Same cause, one step further: the frame moved to the
   other side of a new boundary.
3. **The trigger refused an unpin.** `gallery_anchor_pin_is_final` aborts any UPDATE that would
   clear `user_pinned` on a row that has it. Automation may re-rank a node's anchors and may not
   un-pin one; only a person can, and the panel does that by pinning a different frame.

## What to check

    SELECT node_id FROM gallery_delta WHERE photo_id = '<photo>';

That is the node the photograph is actually in - membership is the delta table, there is no join
table. If it differs from the id the panel sent, reload the tree.

## Why a pin is a veto rather than a vote

Section 6.1: "pinned anchors are authoritative, which gives professionals direct control over the
look of a scene." A pinned frame is used whatever the four ranking terms scored it, because a
photographer looking at a photograph knows something white-balance confidence does not. It survives
every later pass: the store reads pins out before it clears the tree and writes them back onto
whichever node the photograph now belongs to.
