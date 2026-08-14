# ADR-0017 - Burst grouping, the moment model and duplicate policy

**Status:** accepted
**Date:** 2026-08-14
**Phase:** 08 - Smart Burst Grouping & Duplicate Detection
**Supersedes:** nothing. **Amends:** nothing.
**Deciders:** CTO (contract shape, layering), MLL - Vision (grouping objective and gates),
SRC - Core Pipeline (schema, engine), PM (the threshold table and the duplicate wording),
PERF (the storage waiver in section 8)

---

## 1. Context

Photographers shoot in bursts. Six frames of the kiss, fourteen of the bouquet toss,
forty of the dance floor. Every phase from 09 to 29 needs to operate on **moments**
rather than on files, and the reason is not efficiency - it is that the two are
different objects. Rejecting a burst is a moment lost; rejecting individual frames is
tidying. Phase 12's coverage guarantee ("this wedding contains at least one good frame
of the first dance") is a statement about moments, and it is unwritable without them.

Phase 05 gave us a vector per frame and a difference hash. Phase 06 gave us who is in
each frame. Phase 07 gave us what each frame is of. This phase is the first that has to
decide something about *pairs* of photographs, and the decision is consequential in a
way none of the previous three were: a wrong merge hides a keeper the photographer never
sees.

## 2. Six spellings differ from PHASE-08 section 5

Recorded here because the phase document's section 5 is a freeze and these are
deviations from it. The *shapes* are unchanged; the spellings are not.

| Section 5 | This build | Why |
|---|---|---|
| `ImageId` | `PhotoId`, aliased | One type, as phases 05, 06 and 07 all did it |
| `Timestamp` | `scene::Timestamp`, re-exported | A moment's start and a chapter's start are compared directly by phase 12 |
| `CameraId` | a newtype over the catalog's `camera_id` text | See below |
| `Moment` | plus `confidence` and `reasons` | Invariant 2 |
| `DuplicateSet` | plus `reasons` | Invariant 2 |
| - | `MomentOutline`, `MomentService`, `MomentEdit` | See below |

**`CameraId` is text, not `aura_index`'s `CameraId(u32)`.** The index's handle is a
position in one built graph's camera list; it is reassigned every time the graph is
rebuilt and would be meaningless in a stored row. A moment outlives the graph. This is
the same distinction `aura_brain_wedding::index_handle` already draws for scenes, in the
other direction.

**`MomentService` is not in section 5 at all.** Section 5 freezes two data shapes and no
entry point. Seven later phases consume moments, and a contract with no entry point
makes each of them find its own way in - which is precisely the failure the "one way to
ask" rule exists to prevent, three phases running. It is added, and the rule it carries
is stated in the trait's own documentation.

**`MomentEdit` and the `moment_edits` table are not in section 5 either.** Section 13
requires that "manual grouping edits are permanent and undoable", and undoable needs a
journal. It is the same shape as phase 06's `identity_links` and exists for the same
reason.

## 3. The edge score gained a fifth signal, and it is time

PHASE-08 section 2.1 lists the grouping signals: "**time proximity (adaptive)**,
embedding similarity, dHash distance, face-identity overlap, camera identity,
drive-mode/sub-second EXIF evidence". Section 6.2's score has four terms and **time is
not one of them**:

```text
score = 0.55 x (1 - embed_dist) + 0.20 x (1 - dhash_norm)
      + 0.15 x identity_overlap + 0.10 x same_camera
```

In that scoring, time is a *gate* and nothing more: a pair is either inside the adaptive
window or it is not, and among candidates a pair 100 ms apart and a pair 8 s apart are
judged identically.

**The evaluation harness found what that costs.** A ceremony shot at one frame every
eight seconds chains into a single moment for as long as the photographer keeps
shooting: every consecutive pair is inside the eight-second window clamp, and every
consecutive pair looks alike because the altar has not moved. The `slow_ceremony`
fixture is that case, and it came out as one moment where a photographer counts six.

### Decision

The four weights are **unchanged**. Their sum is multiplied by a proximity factor:

```text
score = (four terms) x proximity - scene_penalty
proximity = 1 - 0.20 x (gap / window)
```

A pair at the very edge of its window keeps 80 % of whatever visual evidence it has; a
pair inside a burst keeps essentially all of it.

### Why this is discriminating rather than arbitrary

The window is clamped at both ends (0.7 s and 8 s), and the clamp is what makes the
ratio informative. A burst's window is pinned at the floor while its gaps are 100 ms, so
a burst pair sits at `gap/window` around 0.14. Slow shooting is pinned at the ceiling
while its gaps are 8 s, so a slow pair sits at 1.0. The ratio measures *how tight this
pair is relative to how tight this photographer's shooting could get*, which is what
"time proximity (adaptive)" means.

Measured on the five patterns: bouquet toss 0.14, dance floor 0.30, bracketed detail
0.57, slow ceremony 1.00.

### Alternatives rejected

* **A fifth weighted term**, with the other four scaled down to keep the sum at 1.0.
  Rejected because it changes numbers the phase document states explicitly, and because
  the ablation section 9 asks MLR for would then report four different weights than the
  spec.
* **Tightening the window ceiling below 8 s.** Rejected: the ceiling is section 6.1's,
  and lowering it fragments genuinely slow chapters rather than fixing the scoring.
* **Leaving it and calling the fixture unrealistic.** Rejected after checking: making
  the fixture's frames as different as real ceremony frames are still leaves the score
  above the threshold, because eight seconds of a static altar really is visually
  similar. The gap was in the algorithm.

The scene-mismatch penalty is also an addition in the same sense - section 6.2 names it
in prose ("scene mismatch penalty") without giving it a number. It is 0.06, applied only
when **both** frames carry a known label, so an unclassified wedding pays nothing.

## 4. Grouping thresholds live in their own file

Section 6.2 says thresholds "come from the scene profile". They do not live in
`SceneProfile`, and this is deliberate.

`SceneProfile` is a **frozen contract in `aura-core`** that ten later phases read.
Adding `edge_threshold`, `window_scale` and `max_group` to it would put three grouping
knobs in a type consumed by phases that grade, cull, retouch and lay out albums - none
of which uses them - and would make every one of those crates recompile when a grouping
number is tuned.

So `crates/aura-brain-wedding/config/moment_profiles.toml` is a sibling of
`scene_profiles.toml`, with the same rules: a rationale of at least nine characters or
the file does not load, a version that is written onto every row, an installation
override that falls back to the baseline, and `AURA-ML-5031` for a refusal.

Ten scenes carry an override and twelve take the defaults. The file names which twelve,
deliberately, so a reader can see which numbers were actually argued over.

One validation rule is worth stating here because it prevents a silent half-failure: a
threshold above `W_EMBED + W_DHASH + W_CAMERA` = 0.85 is **refused**, because a pair
with no shared identities cannot reach it and the table would silently disable grouping
for detail shots, venue shots and every wedding where the face pass has not run.

## 5. `moments.segment_id` is nullable

Grouping runs on frames that have embeddings. Segmentation runs on frames that have
scene labels. A wedding that has been embedded but not classified must still group - the
cadence, the hashes and the timestamps are all present - and a `NOT NULL` here would
make phase 08 depend on phase 07 having *succeeded* rather than on phase 07 having run.

The frozen `Moment::segment_id` is non-optional, as section 5 writes it, so the sentinel
`moment::UNPLACED` (the nil UUID) carries "not placed" in the type and NULL carries it in
the column.

## 6. The store reads `faces.identity_id` directly, and that is not a second `PeopleService`

Phase 06's rule is that `PeopleService` is the only way to ask **who** is in a
photograph. `MomentStore::identities_by_photo` reads `faces.identity_id` by SQL, and the
question it asks is different: *do these two frames contain the same people*, answered
with opaque ids that `PeopleService` already assigned.

It holds no template, opens no vault, computes no identity, clusters nothing and cannot
name anybody. Depending on `aura-people` to fetch two integers would put the biometric
crate into the dependency graph of the crate that groups photographs - which is exactly
the arrangement phase 06 built its two-crate split to avoid.

It is the mirror of what phase 07 already does in the other direction:
`aura_people::store::PeopleStore::scene_labels` reads `image_scenes` by SQL rather than
depending on `aura-brain-wedding`.

## 7. A `variant` duplicate set is not stored

Section 6.3 gives three duplicate classes. Only two of them produce rows.

`Identical` and `NearIdentical` **cap the gallery**: at most one frame of such a set may
be delivered. `Variant` says "all frames stay eligible and Phase 12 chooses", which is a
statement that *nothing is constrained* - and the alternatives it would enumerate are
already in the catalog, as the frames of one burst in `moment_images.burst_ix`.

Storing them measured **380 bytes per photograph**, every byte of it restating a column,
against section 11's 200-byte budget.

The class still exists. `duplicate::classify` returns it, the review panel has a heading
for it, and `Moment::suggested_keepers` is the number derived from having several. It is
simply not a row.

## 8. Performance waiver: extra storage per image (PERF + CTO)

Section 11 budgets **200 bytes** per image. The measured figure is **319**. This is a
recorded waiver in the form ADR-0004 and ADR-0007 use, at **340**.

| Field | Value |
|---|---|
| Rule waived | PHASE-08 section 11, "extra storage per image <= 200 B" |
| Measured | 319 B/image, 1,000-frame project, `PRAGMA page_count` before and after |
| Waived at | 340 B/image, in `perf/budgets.toml` |
| Approving | PERF (measurement), CTO (the two structural causes are phase 01's and invariant 2's) |
| Expiry | The waiver expires if phase 01's id format changes, or if a later phase needs the headroom. Re-measure then; do not raise it further without a second waiver. |

### The schema was shaped by the budget before it was measured against it

Four decisions, each against precedent, took the figure from 720 to 319:

1. **`moment_images` carries no `project_id`**, unlike `segment_images` and `faces`,
   both of which justify the redundancy. At 40 characters it is a fifth of the whole
   budget, and every read of this table already joins `moments`.
2. **A burst is a column, not a table.** An id for it would cost 40 bytes per frame to
   name something nothing looks up.
3. **One index on the membership table.** `WITHOUT ROWID` already clusters by
   `moment_id`, so burst order is a range scan plus a sort of at most `max_group` rows.
4. **No `variant` rows** - section 7 above.

### Why the last 119 bytes cannot be recovered here

| Part | B/image |
|---|---|
| `moment_images` rows | 90 |
| its unique index | 85 |
| `moments` rows, over 3.3 frames each | 88 |
| its three indexes | 55 |

**175 of the 319 is text ids at 40 characters each**, which is phase 01's decision and
not this phase's to undo. **46 is `moments.reasons`**, which invariant 2 requires.
Meeting 200 means breaking one of those two, and neither is this phase's to break.

The other three section 11 rows are **met, not waived**: 4,000 images in 10 ms against a
6 s budget, 12,000 in 13 ms against 25 s, and a stack opening in under a millisecond
against 60 ms.

## 9. Consequences

**Good.** Every later phase gets one grouping, with confidence and reasons on every
moment. The duplicate policy is a conjunction of three independent tests, which is a far
stronger claim than any one of them. Nothing in the phase can reject a photograph, and
that is structural: no `culled` column, no rank, and `keep_hint` spelled *hint* in the
contract, the schema, the wire and the panel.

**Costly.** Three more version columns to reason about. A second threshold file for PM
to own. And the storage waiver above.

**Risky.** The `0.55 x (1 - embed_dist)` term is the largest in the score and it reads a
placeholder embedding (phase 05 condition C10). Everything measured in this phase is
measured on authored angles, and no number in it is a claim about how grouping behaves
on a real wedding's pixels. That is condition **C1** in `docs/progress/PHASE-08-EXIT.md`.

## 10. The rule this phase adds

**`MomentService` is the only way to ask what was shot once.** No phase may keep its own
grouping. Two answers to "are these the same shot" is two culling decisions that
disagree, and phase 12's coverage guarantee is written against this one.

Phase 05 wrote this for `SimilarityIndex`, phase 06 for `PeopleService`, phase 07 for
`StoryService`. Fourth phase, fourth time, same reason.
