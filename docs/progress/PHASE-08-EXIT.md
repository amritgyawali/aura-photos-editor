# Phase 08 exit report - Smart Burst Grouping & Duplicate Detection

**Date:** 2026-08-14
**Branch:** `feat/phase-08-burst-grouping-duplicates`
**Gate:** `just phase-08-verify` exits 0
**Verdict:** the phase is implemented and **conditionally** complete. Five conditions are
open, they are listed in section 5, and **C1 is a Sev 2 trigger**.

---

## 1. What shipped

One feature: three thousand loose files become a few hundred moments, and the frames
that repeat a photograph are marked so that only one of them competes for the gallery.

| Area | What landed |
|---|---|
| Migration 8 | `moments`, `moment_images`, `duplicates`, `moment_edits`, and two views |
| `aura-core` | the frozen section 5 contract - `Moment`, `DuplicateSet`, `DuplicateKind`, `CameraId`, `MomentOutline`, `MomentEdit`, `MomentService` - plus `MomentId` |
| `aura-brain-wedding::moments` | the adaptive cadence estimator, the time-windowed similarity graph with scene-conditioned thresholds, deterministic union-find with the over-large split pass, the two-tier burst partition, the three-way duplicate conjunction, cross-camera merging, the editing API and the store |
| Config | `moment_profiles.toml`: ten scenes argued over, twelve on the defaults, every entry with a rationale |
| Fixtures | five burst patterns whose grouping is known by construction, in Rust and in JSON, with a test that fails if the two drift |
| IPC and UI | nine commands, eight types, the stacked moments grid and the duplicate review panel |
| Gate | `aura-cli verify --phase 08`, ten checks, exit 0 |

**Five new error codes**, each with a runbook: `AURA-ML-5028` to `AURA-ML-5032`.

**Two ADRs**: ADR-0017 (the moment model, the grouping design and the storage waiver)
and ADR-0018 (the moments IPC surface).

**No new model.** This is the first phase since 02 to ship none: grouping is arithmetic
over phase 05's vectors and phase 01's timestamps. It is also why section 10.1's gates
are in a better position here than in phases 06 and 07 - and why C1 below is about
somebody else's weights rather than this phase's.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | The grid can switch between 'all frames' and 'moments' views, with correct counts | **met** | gate: `counts reconcile: 52 frames in 14 moments, coverage 100%`; `MomentStack.tsx` and its 24 tests; `coverageSentence` names both denominators |
| 2 | Bursts are grouped the way a photographer would group them on the audit set | **met, with C1** | gate: **ARI 1.000 on all five patterns, 0 mixed groups**; `burst_eval.rs` (16 tests) measures the same numbers against the same ground truth |
| 3 | Near-identical frames are marked with a confidence and reviewable side by side | **met** | gate: `1 set(s) cap the gallery, every one with a confidence`; recall 1.000 / precision 1.000 against the labelled pairs; `DuplicatePanel.tsx` |
| 4 | The same instant captured by two shooters appears as one moment | **met** | gate: `cross-camera: 11 frames from 2 bodies in one moment, 2 sub-groups` |
| 5 | Manual grouping edits are permanent and undoable | **met** | gate: `the photographer's split survived a re-grouping`, then `the split was reversed` |
| 6 | Grouping 4,000 images takes seconds, not minutes | **exceeded** | 10 ms for 4,000, 13 ms for 12,000, against budgets of 6 s and 25 s |

---

## 3. Section 10.1's gates

Measured by `tests/eval/burst_eval.rs` (16 tests) and by the phase gate.

| Gate | Threshold | Result | Against |
|---|---|---|---|
| Burst grouping ARI vs human grouping | >= 0.90 | **1.000** on all five patterns | authored ground truth, known by construction |
| No group mixes two labelled moments | exact | **met**, 0 mixed | the same five patterns |
| Near-identical recall | >= 0.98 | **1.000** | the labelled pair set |
| Near-identical precision | >= 0.95 | **1.000** | the same, with every other pattern as negatives |
| 10 fps burst stays one moment | exact | **met** | `bouquet_toss`, 14 frames |
| A 5-minute dance sequence becomes multiple moments | exact | **met** | `dance_floor`, 5 takes |
| Two cameras on one kiss, one moment, two sub-groups | exact | **met** | `two_shooters`, 11 frames from 2 bodies |
| One-stop-apart exposures are caught | exact | **met** | `bracketed_detail`, 3 pairs |
| Manual split survives re-analysis, undo restores | exact | **met** | gate steps 7 and 8 |
| 4,000 images in <= 6 s, < 200 MB extra | budget | **10 ms** | release, `moment_budgets.rs` |
| Determinism | identical | **met** | `grouping_is_deterministic`; gate: two projects, the same 14 moments |
| A grouper that merges everything fails | - | **met** | ARI 0.000, below the gate |
| A grouper that splits everything fails | - | **met** | ARI 0.000, below the gate |

**Read the "against" column, and read it differently from phases 06 and 07.** In those
phases the deferred gates were about *shipped weights that had not been trained*. Here
there is no model to train: every number above measures the grouping algorithm, and the
algorithm is exactly what will run in the product.

What is *not* measured is what those algorithms do when the embedding underneath means
something. The `0.55 x (1 - embed_dist)` term is the largest in the score and it reads a
placeholder (phase 05 condition C10), so no number here is a claim about a real
wedding's pixels. That is condition C1.

The two last rows are the guard phases 06 and 07 both wrote: a gate that cannot fail is
not a gate, and an ARI is specifically vulnerable to a grouper that merges everything on
a wedding of mostly singletons.

### What the harness and the gate found that review did not

Three real defects, in code that read correctly. Two were only reachable end to end,
which is the argument for having a gate as well as a test suite.

1. **Time proximity was a gate and never evidence.** Section 2.1 lists it first among the
   signals; section 6.2's score has no time term. Without one, a ceremony shot at one
   frame every eight seconds chains into a single moment for as long as the photographer
   keeps shooting - every consecutive pair is inside the eight-second window clamp and
   every consecutive pair looks alike. `slow_ceremony` came out as one moment where a
   photographer counts six. ADR-0017 section 3 records the fix, which leaves section
   6.2's four weights untouched.
2. **EXIF has whole-second resolution, so this phase could not see a burst at all.**
   Every unit test passed and the gate failed. `photo.timeline_time` comes from
   `DateTimeOriginal`, which has no fractional part, so fourteen frames of a 10 fps burst
   carry one timestamp between them; the fraction is in `SubSecTimeOriginal`, which phase
   01 stores separately in `photo.sub_sec`. `moment::sub_sec_ms` reconstructs it, and
   grouping ARI went from 0.000 to 1.000 on two of five patterns. **This is the most
   consequential finding in the phase**, because it is a property of every real camera
   file and no synthetic fixture would ever have exposed it.
3. **A drifting difference hash saturated.** The fixture generator set bits cumulatively,
   so past the sixty-fourth flip every frame had the same hash and the last frames of a
   long burst were classified as copies of each other - one spurious pair in
   `bouquet_toss`, nine in `dance_floor`. The duplicate *precision* gate caught it, which
   is the gate that exists for exactly this direction of error.

---

## 4. Section 11's budgets

Measured in release on the development machine (Intel i5-10300H, 8 GB, Win 11), asserted
by `crates/aura-perf/tests/moment_budgets.rs`.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Grouping 4,000 images | <= 6 s | **10 ms** | **met**, by a factor of 600 |
| Grouping 12,000 images | <= 25 s | **13 ms** | **met**, and linear-ish: 3x the frames cost 1.3x the time |
| Moment stack expand/collapse | <= 60 ms | **under 1 ms** | **met** |
| Extra storage per image | <= 200 B | **319 B** | **waived at 340** - ADR-0017 section 8 |

Three of four met, and the three that are met are met by two to three orders of
magnitude. The cause is structural rather than lucky, and it is the same one phase 07
had: grouping reads `embeddings`, `descriptors`, `photo`, `faces` and `image_scenes` and
**opens no image file**. Section 6.2's "never all-pairs" does the rest - the candidate
sweep is bounded per frame by the adaptive window and by `MAX_NEIGHBOURS`, so a
4,000-frame wedding scores about sixty thousand pairs rather than eight million.

### The storage row, stated plainly

The budget is 200 bytes per image. The measured figure is 319, and the waiver is at 340.

The schema was **shaped by the budget before it was measured against it**, and four
decisions - each against this project's own precedent - took the figure from 720 to 319:
no `project_id` on `moment_images`, a burst as a column rather than a table, exactly one
index on the membership table, and no rows at all for `variant` duplicate sets.

The remaining 119 bytes are **175 bytes of 40-character text ids** (phase 01's decision)
and **46 bytes of `moments.reasons`** (invariant 2's requirement), amortised. Meeting
200 means breaking one of those two, and neither is phase 08's to break. ADR-0017
section 8 carries the decomposition and the expiry condition.

### Phase 06's clustering budget still does not reproduce

`people_budgets::clustering_a_full_skeleton_stays_inside_the_budget` fails on this
machine at 21.7 s against a 12 s budget, exactly as `docs/progress/PHASE-07-EXIT.md`
section 4 records. Phase 07 ruled out its own involvement by measurement; nothing in
phase 08 touches `aura_vision::face::cluster` either. It is carried forward again,
unrepaired, because repairing a phase 06 claim from inside phase 08 is not this phase's
call to make. **Owned by PERF, against phase 06, and it has now been carried twice.**

---

## 5. Open conditions

### C1 - the embedding underneath is a placeholder (**Sev 2 trigger**)

The largest term in section 6.2's score is `0.55 x (1 - embed_dist)`, and it reads phase
05's `wedding_embedding` 1.0.0, which carries no wedding semantics. On a real photograph
its distances describe a random projection.

Everything around it is real and measured: the cadence estimator with its clamps and its
per-camera medians, the sub-second reconstruction, the bounded candidate sweep, the
proximity multiplier, the scene-conditioned thresholds, the scene-mismatch penalty, the
deterministic union-find, the over-large re-cluster and its cut-on-gap guarantee, the
four-tier burst evidence, the three-way duplicate conjunction with its skip rule and its
confidence arithmetic, the cross-camera overlap and medoid tests, the split and merge
validation, the undo journal, the store's lock rules, the IPC surface and the two panels.

**No later phase may claim a grouping quality result on real photographs until this
closes.** It closes with phase 05's C10, not separately: a trained grouping cannot exist,
because there is nothing here to train.

What *will* survive that training, and is worth saying because it is unusual: the
**performance** numbers. The vectors are 512 halves whatever produced them and
`cosine_distance` does not know the difference, so the three met budget rows are claims
about the shipped code rather than about the fixtures.

### C2 - there is no human burst-grouping ground truth

Section 9 gives DATA "human burst-grouping ground truth on fixtures plus adversarial
cases". What exists is **authored** ground truth: five patterns whose answer is stated in
`tests/fixtures/labels/bursts_*.json` with a `why` a photographer would recognise, and a
test that fails if the Rust and JSON copies drift apart.

That is a real artefact and it is not the same thing. Nobody has grouped a real
wedding's frames by hand and compared. The ARI of 1.000 is an ARI against a set that was
written to be gradeable, and its value is that it would catch a regression - not that it
predicts the number on a shoot.

### C3 - the chapters, stacks and duplicate sets are not audited by a human

Section 9 gives QAIQ "eyeball 500 groups: any group mixing two different moments is a
bug". There are no real weddings here. `no_group_mixes_two_labelled_moments` asserts the
property on five synthetic patterns; nobody has looked at a stack for a wedding they
attended.

This is the condition that matters most for the *product* rather than for the code, and
it is a straightforward day's work the moment there is a real wedding to look at.

### C4 - phase 06's two face signals are not wired in

`PassContext::subject_focus` and `PassContext::face_overlap` sit at their documented
neutrals, so:

* the `0.15 x identity_overlap` term is exercised only by tests that construct identity
  sets directly - on a real wedding it contributes zero, and the grouping is decided by
  appearance, hashes, cameras and cadence;
* `duplicate::keep_hint` chooses on edge energy rather than on face sharpness, and says
  so in the set's reasons;
* the face-box overlap test in the near-identical conjunction is **skipped**, which costs
  the set confidence rather than passing for free.

The reason is a missing API rather than an omission. Filling them means either a
`PeopleService` call per frame - four thousand queries on a wedding - or this crate
recomputing them from `faces`, which is the rule phase 06 built its two-crate split to
prevent. `PeopleService` needs a bulk accessor, and adding one is a phase 06 contract
change with its own ADR.

**It is not blocking**, and the degradations are all in the safe direction: a skipped
face test makes a near-identical claim *harder* to make, not easier.

### C5 - no perceptual comparison against the named competitors

Section 10.2 asks for a blind A/B against Aftershoot and Narrative Select at >= 60 %
preference. Neither is installed here and the comparison needs a panel.

---

## 6. Carried forward from earlier phases

Phase 02's three exit conditions are still open and are carried again: real camera files,
a photographed ColorChecker, and a three-OS CI run. **The first real camera file is a Sev
2 trigger that reopens phase 02's criteria whatever phase is in flight** (ADR-0006).

It is worth noting that phase 08 has an unusually direct interest in that condition. The
sub-second defect in section 3 was found because the gate wrote timestamps the way phase
01 writes them; what it *cannot* establish is how many real bodies write
`SubSecTimeOriginal` at all, or with how many digits. `docs/runbooks/AURA-ML-5030.md`
names that as the first thing to check when a wedding comes out as all singletons.

Phase 05's condition C10 - the perceptual embedding is a placeholder - is unchanged and
phase 08 **depends on it directly**; see C1.

Phase 06's conditions are unchanged. C1 (the face models are placeholders) is the reason
C4 above is not merely an API gap: even wired in, the identity term would carry no
information until phase 06's models are trained.

Phase 07's conditions are unchanged. Phase 08 does not depend on C1: an unclassified
wedding groups on the default profile and pays no scene-mismatch penalty, which is why
`moments.segment_id` is nullable.

---

## 7. Rollback

| Switch | How |
|---|---|
| Feature off | Do not call `group_moments`. Nothing else in the product requires `moments`; `MomentOutline::coverage` reports 0.0, `MomentService::moment_of` returns `None`, and the grid stays on the all-frames view it has had since phase 01. |
| Config rollback | The shipped `moment_profiles.toml` is embedded in the binary. An installation override that will not load falls back to it with a logged refusal; the *embedded* file failing to load is `AURA-ML-5031` and halts, which is the correct direction for a threshold table to fail in. |
| Regrouping | `Moments::group` rebuilds every unlocked moment from the catalog and preserves every locked one. It touches no pixel, no embedding and no scene label. |
| Migration reversible | Yes. The down migration is six drops, written out at the top of `0008_moments.sql`. It costs the photographer's manual splits and merges, which is the one thing here that cannot be recomputed - so the runbook says to export `moment_edits` first, and the table exists partly so that export is one statement. |
| Threshold rollback | `profile_ver` is on every `moments` row. A phase that acted on version 1's groupings and a phase that acted on version 2's are distinguishable after the fact, which is what makes a threshold change auditable rather than merely reversible. |
| Version rollback | Three columns - `embed_ver`, `group_ver`, `profile_ver` - and `AURA-ML-5028` when they disagree with this build. Two vintages are never compared; the outline reports the lowest present. |

---

## 8. What phase 09 inherits

Five rules, and every later phase inherits them.

- **`MomentService` is the only way to ask what was shot once.** No phase may keep its own
  grouping. This is phase 05's rule for `SimilarityIndex`, phase 06's for
  `PeopleService` and phase 07's for `StoryService`, a fourth time and for the same
  reason: two answers to "are these the same shot" is two culling decisions that
  disagree, and phase 12's coverage guarantee is written against this one.
- **A grouping is evidence; the deciding phase owns the cull.** Nothing in
  `moments` rejects, ranks or deletes a frame, and there is no column, field or command
  on any surface that would. `DuplicateSet::keep_hint` is spelled *hint* in the contract,
  the schema, the wire and the panel, and section 6.3 calls it provisional. Phase 05 wrote
  this about distances and phase 07 about scene tolerances; this is the same rule about
  groupings.
- **Three version columns, because they invalidate three different things.** `embed_ver`
  invalidates every distance and therefore every edge, `group_ver` invalidates the graph
  construction, and `profile_ver` invalidates the thresholds those edges were compared
  against. `AURA-ML-5028` exists so a comparison across any of them never happens
  silently. Fourth phase, fourth version-drift code.
- **Report coverage when you report a result, and say what the denominator is.** A moment
  list over a half-embedded wedding describes half a wedding, and `MomentOutline::coverage`
  is how a caller finds out. Phase 08's refinement: the denominator is **groupable**
  frames, not photographs, because a frame with no embedding is a phase 05 gap and
  reporting it as a phase 08 failure sends somebody looking in the wrong place.
- **A photographer's grouping is unbeatable, and both sides of a split are locked.**
  `moments.user_locked = 0` is inside the `DELETE` a re-grouping starts with, and the
  frames of a locked moment are *subtracted from the pass's input* rather than reconciled
  afterwards - the pass cannot contradict a decision already made, because it never sees
  those frames.

And two things phase 09 should know before it starts:

**`Moment::suggested_keepers` is advice, and `SceneProfile::keeper_rate` is the band.**
The first is derived from diversity - one keeper for a bracketed static subject, up to
three when the action evolves - and it is a statement about what the *evidence* supports.
The second is phase 07's statement about what a photographer expects. Phase 12 reconciles
them; phase 09 should read neither as a target.

**A frame carrying `suppressed` is not a rejected frame.** It means a duplicate set caps
delivery to one, and it is drawn in the interface as an explanation. A phase that treats
it as a cull has made phase 12's decision on phase 12's behalf, three phases early.
