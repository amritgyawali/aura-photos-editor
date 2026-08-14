# Phase 07 exit report - Wedding Scene AI & Story Timeline Segmentation

**Date:** 2026-08-14
**Branch:** `feat/phase-07-wedding-scene-story-ai`
**Gate:** `just phase-07-verify` exits 0
**Verdict:** the phase is implemented and **conditionally** complete. Five conditions are
open, they are listed in section 5, and **C1 and C5 are Sev 2 triggers**.

---

## 1. What shipped

One feature: the app reads the wedding as a story. It labels every photograph's scene
and splits the day into ordered chapters with confidence, and every threshold in every
later phase is now a function of that label.

| Area | What landed |
|---|---|
| Migration 7 | `image_scenes`, `segments`, `segment_images`, `scene_profiles`, and two views |
| `aura-core` | the frozen section 5 contract - `SceneId`, `AttrFlags`, `RitualId`, `ChapterId`, `SceneResult`, `Segment`, `SceneProfile`, `StoryOutline`, `StoryService` - plus `SegmentId` |
| `aura-brain-wedding::scene` | the multi-head classifier on the frozen phase 05 trunk, the sixteen context features, the attribute decoder with its four measured overrides, the tradition-conditioned ritual head with two abstention mechanisms, the ritual taxonomy loader and the scene profile registry |
| `aura-brain-wedding::story` | HMM smoothing over nine chapters, PELT change-point detection with a log-space penalty search, the merge pass, medoid key frames with a three-step relaxation, the segment store and `Story` - the one implementation of the frozen `StoryService` |
| Config | 22 scene profiles each with a signed-off rationale, 48 rites across five traditions |
| Models | `scene_classifier`, `ritual_classifier` - signed into `models.lock` with two model cards |
| Cloud | the `SegmentNaming` **policy**: sixteen calls per wedding, least-confident first, locked chapters never priced, phase 04's authority rule enforced |
| People | `PeopleStore::scene_labels` - half of phase 06's condition C3 closes |
| IPC and UI | nine commands, thirteen types, the story timeline with chapter strip, boundary editor and review flags |
| Gate | `aura-cli verify --phase 07`, thirteen checks, exit 0 |

**Six new error codes**, each with a runbook: `AURA-ML-5022` to `AURA-ML-5027`.

**Two ADRs**: ADR-0015 (the taxonomy and the segmentation design) and ADR-0016 (the story
IPC surface).

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | A freshly analysed wedding shows a correct, ordered chapter strip with counts and durations | **met, with C1** | gate: `segment: 10 chapters at penalty 0.0084, 2 hard boundaries, coverage 100%`; the strip's logic is unit-tested in `ui/src/components/story/Timeline.test.tsx` (23 tests) |
| 2 | Every image carries a scene label, attributes and confidence, visible in the Explain panel | **met** | `image_scene` returns the label, the padded top-3, the attribute *names*, `attributesMeasured` and the source; gate: `1692 scene rows written` |
| 3 | Hindu, Nepali, Christian and Muslim fixture weddings all produce correct ritual labels or honest abstentions | **met for the abstentions, C1 for the labels** | `ritual_abstention_beats_a_wrong_tradition`; every rite the three fixtures name is declared by a shipped taxonomy; the shipped head names nothing |
| 4 | Editing a boundary or renaming a chapter persists through re-analysis | **met** | gate: `decision: the photographer's chapter survived a re-analysis (1 locked of 10)`; and `override: a hand-set scene survived a re-classification (0 rows changed)` |
| 5 | `scene_profiles.toml` drives measurable behaviour differences in later phases, proved by a fixture test | **met at this phase's boundary** | `the_worked_examples_from_section_six_three_hold` asserts section 6.3's three named behaviours directly. The *later phase* half cannot be proved here: phase 09 measures noise and phase 12 acts on it, and neither exists |
| 6 | Low-confidence chapters are flagged for review rather than silently guessed | **met** | `Segment::needs_review` at 0.75, `StoryOutline::needs_review`, `reviewPrompt` in the timeline; gate: `cloud: 4 of 10 chapters worth asking about` |

---

## 3. Section 10.1's gates

Measured by `tests/eval/scene_eval.rs` (18 tests) and by the phase gate.

| Gate | Threshold | Result | Against |
|---|---|---|---|
| Scene top-1 accuracy | >= 0.92 | **0.94** on all three fixtures | synthetic posteriors driven at 0.94 |
| Per-class accuracy, classes with > 200 samples | >= 0.85 | **deferred - C2** | needs a labelled corpus |
| Median boundary error | <= 45 s | **0 s** on all three fixtures; **0 s** in the gate | synthetic weddings with known boundaries |
| Chapters per wedding | 6 to 20 | **10, 7, 8** | the three reference weddings |
| Ritual F1 per tradition | >= 0.85 | **deferred - C1** | the head is a placeholder |
| No chapter sequence violates the transition matrix | - | **met** | `no_chapter_sequence_violates_the_transition_matrix` |
| Smoothing does not make chapter labels worse | - | **met** | `smoothing_improves_on_the_raw_classifier`, at a driven 0.80 |
| A travel gap is always a boundary | - | **met** | the Hindu fixture's 35-minute drive |
| Two-shooter interleaving does not fragment chapters | - | **met** | `two_shooters_do_not_fragment_the_timeline` - identical chapter count interleaved and solo |
| User overrides survive full re-analysis | - | **met** | gate steps 8 and 9 |
| Determinism | byte-identical | **met** | `the_whole_pipeline_is_deterministic`; gate: two projects, the same 10 chapters |
| A useless classifier fails the accuracy gate | - | **met** | `the_gate_rejects_a_useless_classifier` |

**Read the "against" column.** Every number in it is a real measurement of the
*algorithms* - the Viterbi path, the emission mapping, the penalty search, the merge
pass, the abstention rules, the confidence arithmetic - on ground truth whose answer is
known by construction. None of them is a measurement of the *shipped weights*, which
carry no scene semantics at all. That is condition C1, and it is why
`the_gate_rejects_a_useless_classifier` exists: it proves the harness would fail a model
that had learned nothing.

### What the harness found that review did not

Three real bugs, in code that read correctly. They are the argument for building the
fixtures before the gates rather than after.

1. **The penalty search never reached its own range.** Linear bisection of `0.0005..40`
   spends its first ten steps between 40 and 0.04; the Nepali fixture's answer is 0.008.
   It fell back to gap-only segmentation and produced three chapters against a
   six-chapter floor. The search is logarithmic now.
2. **Masking made the ritual head abstain more, not less.** Zeroing another tradition's
   slots without renormalising left the distribution summing to under one, so the
   confidence floor rejected an answer that establishing the tradition should have made
   easier. Exactly backwards.
3. **The storage estimate was 25 % low.** The migration claimed 330 bytes per image; the
   catalog said 410 against a 400-byte budget.

---

## 4. Section 11's budgets

Measured in release on the development machine (Intel i5-10300H, 8 GB, Win 11), asserted
by `crates/aura-perf/tests/scene_budgets.rs`.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Scene + attributes for 4,000 images | <= 35 s | 0.002 ms per frame, so **~8 ms** for 4,000 | **met** |
| Segmentation + smoothing | <= 2 s | 35 ms on 1,692 frames; **125 ms** on 4,000 | **met** |
| Timeline UI open | <= 200 ms | **2 ms** | **met** |
| Extra storage per image | <= 400 B | **362 B** | **met** |

**All four rows are asserted and none is waived.** That is the first time since phase 02
this has been true, and the cause is structural rather than lucky: the scene heads sit on
the frozen phase 05 embedding rather than on pixels, so nothing in this phase needs a GPU
to be measured honestly. Phase 06's two waived rows (ADR-0013 section 6) are unaffected
and remain waived.

The 35-second budget deserves one sentence, because the measured figure is three orders
of magnitude inside it and that looks like a mistake. It is not: the classifier is one
528x256 matrix multiply plus two small ones, about 300 kFLOP per frame. **The budget is
dominated by reading four thousand embeddings out of SQLite**, which is what
`scene::pass::BATCH` is sized for, and the per-frame figure above measures the feature
assembly, the decode, the abstention and the write rather than the arithmetic alone.

### One phase 06 budget does not reproduce, and it is not phase 07's doing

`cargo test --release -p aura-perf --all-targets` is **not** fully green on this machine.
`people_budgets::clustering_a_full_skeleton_stays_inside_the_budget` fails:

```
identity_cluster_skeleton: 21671 ms over 1 units, budget 12000 ms
```

`docs/progress/PHASE-06-EXIT.md` records 2.1 s for the same measurement. The difference is
a factor of ten and it is recorded here rather than quietly repaired, because repairing a
phase 06 claim from inside phase 07 is not this phase's call to make.

**It is not caused by phase 07.** The dependency was ruled out by measurement rather than
by argument: with `aura-brain-wedding` and `rusqlite` removed from `aura-perf`'s
dev-dependencies and `scene_budgets.rs` taken out of the crate entirely, the same test
measures **21,671 ms** - within noise of the 22,848 ms it reports with them present.
Nothing in this phase touches `aura_vision::face::cluster`, `MAX_SKELETON` or the
synthetic templates.

What the arithmetic suggests, for whoever picks this up: exact average linkage over the
4,096-face skeleton is 16.7 M pairwise 512-d cosine distances, about 8.6 GFLOP. Twenty-two
seconds is roughly 0.4 GFLOP/s, which is a plausible single-threaded scalar figure for
this processor and is *not* consistent with 2.1 s. The 2.1 s in the phase 06 report looks
more likely to be a measurement of a smaller skeleton than a regression since.

**Action:** owned by PERF, against phase 06, before phase 08's gate. Either the budget is
wrong, the phase 06 measurement was taken at a different cap, or the clustering pass has
regressed for a reason predating this branch. All three are answerable in an hour with a
profiler and none of them is answerable by changing a number here.

---

## 5. Open conditions

### C1 - the two models are placeholders (**Sev 2 trigger**)

`scene_classifier` and `ritual_classifier` 1.0.0 have the architecture of a multi-head
adapter and a tradition-conditioned rite classifier, and none of their training. **The
shipped classifier's posterior over a real photograph describes a random projection of
its embedding, and the shipped ritual head names no rite.**

Everything around them is real and measured: the 528-wide feature assembly, the sixteen
context slots with their documented neutrals, the cyclic hour encoding, the softmax
decode, the top-3 ordering, the margin rule, the attribute correction from EXIF and
luminance, the tradition mask and its renormalisation, the two abstention mechanisms, the
Viterbi smoother, the hand-authored transition matrix, the fused change signal, the
log-space penalty search, the merge pass, the medoid key frames, the profile registry,
the store's three user-override guards, the cloud policy, the IPC surface and the
timeline.

**No later phase may claim a quality result that depends on scene classification being
accurate until this closes.** The first trained model reopens section 10.1's accuracy and
ritual gates against photographs.

### C2 - there is no labelled wedding data

Section 8 step 2 asks for "the fixture weddings and an expansion set covering four
traditions (DATA)". That work has not been done and it is the blocker for C1.

What exists instead is everything except the labels: `ml/models/scene/dataset.py`
assembles the left-hand side from a catalog, enforces wedding-level splits and refuses a
frame-level one, reports class balance and names the classes below section 10.1's
200-sample floor. `train_multihead.py --plan` and `train_ritual.py --plan` state the
training design precisely enough to be argued with *before* eight days of labelling are
spent against the wrong loss - including the three numbers a reviewer should push back
on.

Section 10.1's per-class gate is deferred to this.

### C3 - the chapters are not audited by a human

Section 9 gives QAIQ "human review of chapters on 20 weddings across traditions". There
are no real weddings here. The boundary error is measured against synthetic timelines
whose answer is known by construction, the chapter band is asserted on all three
reference weddings, and the two-shooter case has its own regression - but nobody has
looked at a chapter strip for a wedding they attended.

### C4 - no perceptual comparison against the named competitors

Section 10.2 asks for a blind A/B against FilterPixel and Aftershoot at >= 60 %
preference. Neither is installed here and the comparison needs a panel.

### C5 - no per-tradition accuracy published (**Sev 2 trigger**)

Section 12's first failure mode is cultural blind spots, and this is the phase where that
risk is concentrated. Both model cards have a fairness section and both say the same
thing: the number is **not published and not approximated**.

The shape of the risk is different from phase 06's C5 and worth stating separately. A
scene classifier does not recognise people, so the disparity is not demographic - it is
**cultural**. A head trained mostly on Western weddings reads a mandap as `stage` +
`crowd` + `other` and a nikah as `speeches`. That is precisely the gap section 1 claims
as a competitive moat, which means an unmeasured version of it is a claim the product
cannot support.

What is needed, and is written into both cards:

- **per-tradition top-1 accuracy**, on a set with balanced coverage of the five shipped
  traditions, and per-tradition **recall on the pivotal scenes** (`SceneId::is_pivotal`)
  rather than overall accuracy, which hides the failure;
- for the ritual head, **F1 and abstention rate per tradition, both**. A head that
  abstains on every Nepali rite and names every Christian one has a fine average F1 and
  is useless to half the customers this product is built for;
- the same figures on the **night** and **mixed-light** subsets, since three of the five
  traditions hold their central rites after dark;
- an agreed maximum disparity, decided with PM and a consultant rather than reported
  without a threshold.

`ml/models/scene/eval_scene.py` computes every one of those figures today and
**deliberately refuses to pass or fail on them**, printing the reason instead.

The **mitigation is implemented rather than promised**: where the evidence is weak the
decoder abstains - `SceneId::Unknown` with the top-3 intact, and `None` with a reason for
a rite - which turns a cultural failure into a visible gap and a `needs_review` flag
rather than a confident wrong label in a client's timeline. The margin rule specifically
turns "cannot tell which tradition's name to use" into silence rather than into the
majority tradition's name, which is the direction a fairness failure should fail in.

---

## 6. Carried forward from earlier phases

Phase 02's three exit conditions are still open and are carried again: real camera files,
a photographed ColorChecker, and a three-OS CI run. **The first real camera file is a
Sev 2 trigger that reopens phase 02's criteria whatever phase is in flight** (ADR-0006).

Phase 05's condition C10 - the perceptual embedding is a placeholder - is unchanged, and
phase 07 **depends on it directly**. The scene heads are adapters on that trunk; a
trained scene head on an untrained trunk would be a linear probe of a random projection.
C10 and C1 close together or not at all.

Phase 06's conditions:

- **C1** (the face models are placeholders) is unchanged. Phase 07 does not depend on it:
  the face count and couple-presence context features substitute documented neutrals when
  the face pass has not run, and the key-frame filter relaxes to its `Any` step and says
  so.
- **C3** (couple identification is not audited) is **half closed**. `PeopleStore::scene_labels`
  now feeds real coarse labels into the co-occurrence graph, so `RoleOutcome::scene_starved`
  is false on a classified wedding and `SCENELESS_CONFIDENCE_CEILING` stops capping the
  couple decision at 0.62. The gate reports `people: 1249 frames carry a coarse label the
  couple contest can use, 890 of them ceremony`. The other half - an audit on twenty real
  weddings - stays open, because it needs weddings.
- **C2**, **C4** and **C5** are unchanged.

---

## 7. Rollback

| Switch | How |
|---|---|
| Feature off | Do not call `classify_scenes`. Nothing else in the product requires `image_scenes`; `StoryOutline::coverage` reports 0.0, `StoryService::scene` returns `None`, and every consumer falls back to its non-scene path. Phase 06's couple contest returns to `scene_starved` and its 0.62 ceiling. |
| Model rollback | `models.lock` pins by digest; the registry keeps the previous version until a new one has completed one real inference (`AURA-ML-5009`). A `MODEL_VER` bump makes every `image_scenes` row stale, so the next pass re-classifies. Two versions are never compared - `AURA-ML-5022`. |
| Config rollback | The shipped `scene_profiles.toml` and the five taxonomies are embedded in the binary. An installation override that will not load falls back to them with a logged refusal; the *embedded* file failing to load is `AURA-ML-5024` and halts, which is the correct direction for a threshold table to fail in. |
| Migration reversible | Yes. The down migration is six drops, written out at the top of `0007_scenes.sql`. Unlike migration 6's rollback this one is cheap: every row here can be recomputed from data the product still has. |
| Segmentation rollback | `Story::segment` rebuilds every chapter from the stored labels and preserves every `user_locked` one. It touches no pixels and no scene label. |
| Threshold rollback | `profile_ver` is on every `scene_profiles` row. A phase that acted on version 1's tolerances and a phase that acted on version 2's are distinguishable after the fact, which is what makes a profile change auditable rather than merely reversible. |

---

## 8. What phase 08 inherits

Five rules, and every later phase inherits them.

- **`StoryService` is the only way to ask what a photograph is of.** No phase may keep
  its own scene classifier or its own idea of where the ceremony was. This is phase 05's
  rule for `SimilarityIndex` and phase 06's for `PeopleService`, a third time, and for
  the same reason: two answers to "is this the ceremony" is two thresholds that disagree.
- **A profile is evidence about a scene; the deciding phase owns the action.** Nothing in
  `aura-brain-wedding` culls, grades or crops. `SceneProfile::max_acceptable_noise` is a
  tolerance phase 09 measures against and phase 12 acts on, and this crate has no opinion
  about either. Phase 05 wrote the same rule about distances and phase 06 about faces.
- **Four version columns, because they invalidate four different things.** `model_ver`
  invalidates the posterior, `preprocess_ver` invalidates the context features,
  `taxonomy_ver` invalidates the rite's name, and `embed_ver` invalidates everything
  because the trunk is underneath all of it. `AURA-ML-5022` exists so a comparison across
  any of them never happens silently.
- **Report coverage when you report a result.** A story drawn over a 40 %-classified
  wedding is a story about 40 % of a wedding, and `StoryOutline::coverage` is how a
  caller finds out. Third phase, third time.
- **A photographer's chapter is unbeatable, and a boundary belongs to two chapters.**
  `segments.user_locked` and `image_scenes.source = 'user'` are checked inside the
  statements that would overwrite them. Moving a boundary locks *both* sides, because
  locking one would let the next re-analysis move it back from the other.

And one thing phase 08 should know before it starts: **`SceneId::is_pivotal` and
`SceneProfile::must_cover` are the vocabulary for coverage guarantees, and the list is
deliberately short.** Eleven scenes are pivotal and fifteen carry a coverage guarantee. A
guarantee on everything is a guarantee on nothing, and the list is the most argued-over
thing in `scene_profiles.toml` for that reason.
