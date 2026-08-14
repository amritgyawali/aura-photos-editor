# AURA-ML-5030 - The grouping produced an implausible number of moments

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence in the moments view's header, above a grouping that is still there and still usable. Every stack can be split or merged by hand, and nothing was deleted.

## What actually happened

Section 0's mission is that roughly 3,000 files become 700 to 1,100 moments - between 2.7 and 4.3 frames each. `graph::PLAUSIBLE_MEAN_SIZE` is a **much** wider band than that, 1.0 to 12.0, and the width is the point: a photographer who shoots one deliberate frame per pose genuinely has one frame per moment, and a sports-style shooter genuinely has twelve. What the code can honestly say is that something outside 1 to 12 is worth telling somebody about.

Two directions, two causes.

### Too many moments (mean size at or near 1.0)

Everything came out as a singleton. In order of likelihood:

1. **`photo.sub_sec` is empty and the cadence collapsed.** This is the big one, and it is the defect the phase 08 gate found. EXIF's `DateTimeOriginal` has whole-second resolution, so `timeline_time` alone cannot distinguish fourteen frames of a 10 fps burst - they all carry the same stamp. `moment::sub_sec_ms` reconstructs the fraction from `SubSecTimeOriginal`. A body that writes no sub-second tag, or a file that has been through a metadata-stripping pipeline, has nothing to reconstruct from, and every burst looks like one instant followed by a one-second gap.
2. **The embeddings are missing or degenerate.** `SELECT COUNT(*) FROM embeddings WHERE project_id = ?` against the photograph count. A zero vector sits at distance 1.0 from everything and joins nothing.
3. **A threshold table was edited upward.** `moment_profiles.toml` at an `edge_threshold` near the reachable ceiling groups almost nothing.

### Too few moments (mean size near or above 12)

Whole runs merged. In order of likelihood:

1. **Every frame looks alike to the embedding.** The shipped embedding is a placeholder (phase 05 condition C10) and carries no wedding semantics; on a real wedding its distances describe a random projection. This is the expected cause until that closes, and it is why no quality claim in this phase depends on it.
2. **Timeline times are wrong.** A card whose camera clock was never aligned lands its frames in one dense cluster. `SELECT MIN(timeline_time), MAX(timeline_time) FROM photo WHERE project_id = ?`.
3. **A threshold table was edited downward.**

## What AURA does automatically

**Writes the moments anyway**, and that is deliberate. A grouping nobody trusts is still better than a grid of three thousand loose files: every stack can be split, the frames are all there, and the alternative - refusing to group - leaves the photographer with strictly less. The code exists so the Problems panel can say what happened rather than leaving somebody to notice.

Invariant 9: a typed error, a fallback path, and a telemetry event.

## Operator steps

1. Read the mean size in the message. It says which direction the failure is in, and the two directions have disjoint causes.
2. For "too many": `SELECT COUNT(*) FROM photo WHERE project_id = ? AND sub_sec IS NULL;` - a high count against a project that should contain bursts is cause 1, and the answer is that this camera cannot be burst-grouped at better than one-second resolution. Say so rather than tuning around it.
3. `SELECT COUNT(*) FROM embeddings WHERE project_id = ?` against the photograph count, for cause 2.
4. Check `moments.profile_ver` against the shipped `moment_profiles.toml` version. A mismatch means an installation override is in force.
5. Re-run the grouping pass after any fix; it is seconds.

## When this is not the problem

An elopement with sixty photographs has a mean size near 1 and is not a fault. The band is a statement about a wedding day's shape, and a small shoot legitimately falls outside it - which is why this degrades rather than halts.

## Related

* `AURA-ML-5026` - the same shape for chapters: a plausible-count band, a fallback, and a wedding that stays usable.
* `AURA-ML-5032` - individual frames that could not be placed at all.
