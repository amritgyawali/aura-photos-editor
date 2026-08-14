# ADR-0015 - The wedding scene taxonomy and story segmentation

**Status:** accepted
**Date:** 2026-08-13
**Phase:** 07 - Wedding Scene AI & Story Timeline Segmentation
**Deciders:** CTO, ML Lead - Vision, ML Research Engineer, Senior Engineer - Core Pipeline,
Product Manager

## Context

Phase 07 ships the scene graph that makes every later threshold scene-aware. Invariant 7 -
"no threshold is global; every threshold is a function of the detected scene and subject
role" - has been an unenforceable promise for six phases, because there has been nothing
to condition on. This phase is where it becomes a lookup.

Section 5 of `docs/plan/phases/PHASE-07-WEDDING-SCENE-STORY-AI.md` freezes three shapes -
`SceneResult`, `Segment` and `SceneProfile` - and section 8 says to freeze them before
coding. This ADR records where the implementation spells them differently, where the
phase document and an earlier phase disagree, and the six design decisions that the
frozen shapes do not by themselves determine.

The rule from `docs/plan/CLAUDE.md` section 1 applies throughout: **the newer ADR wins
over older prose, and this file wins over the phase document where the two differ.**

## Decision

### 1. The frozen contract lives in `aura-core`, not in `aura-brain-wedding`

`crates/aura-core/src/contract/scene.rs` holds `SceneId`, `AttrFlags`, `RitualId`,
`ChapterId`, `SceneResult`, `Segment`, `SceneProfile`, `EditIntent`, `StoryOutline` and
the `StoryService` trait.

The argument is ADR-0013's, applied to a second vocabulary. The list of phases that
consume a scene label is longer than the list that consumes an identity: 09 (technical
quality), 10 (emotion), 11 (composition), 12 (culling and coverage), 13 (the ledger), 15
to 17 (editing intent), 18 to 22 (masks and retouch), 25 (gallery consistency), 27 (QC)
and 29 (curation). Every one of them needs the word `ceremony` and the tolerances that
hang off it. None of them needs the classifier, the change-point detector or the ONNX
session.

Putting the vocabulary in `aura-core` means a phase can condition a threshold on a scene
without linking the crate that produces one. `aura-core` still depends on no other
workspace crate; `crates/aura-core/tests/no_workspace_deps.rs` asserts it.

### 2. Five spellings differ from the phase document

All five are deliberate and all five are here rather than in a comment somebody will
delete.

* **`ImageId` is `PhotoId`.** One type, aliased, exactly as `aura_index` and
  `aura_core::contract::people` already do it. A conversion between a similarity query, a
  subject query and a scene query is a conversion that can disagree.
* **`Timestamp` is `i64` milliseconds since the Unix epoch.** The catalog stores RFC 3339
  text; `aura_index::store::parse_rfc3339_ms` is the one parser in the product and phase
  06's identity timelines already run on its output. A second time representation for
  segments would let a boundary and an appearance disagree about when a photograph was
  taken, which is precisely the 45-second measurement this phase is graded on.
* **`top3` is `[SceneScore; 3]`, not `[(SceneId, f32); 3]`.** A named pair serialises to
  an object with named fields instead of a two-element array, which is what the IPC
  surface and the Explain panel need. The arity is unchanged.
* **`AttrFlags` is a hand-rolled `u16` newtype, not a `bitflags!` macro.** The workspace
  has no `bitflags` dependency and adding one to `aura-core` - the crate that is forbidden
  to depend on anything - to save forty lines of `const fn` is a bad trade. Fourteen
  named bits, two spare, `Serialize` as a `u16`.
* **`reasons: Vec<String>` on `Segment` is capped at `Segment::MAX_REASONS`.** Invariant 2
  requires reasons; nothing requires an unbounded number of them, and a segment row is
  written once per chapter per re-analysis. Six, the same cap the cloud schema uses.

### 3. Twenty-two scenes in code, nine chapters in code, rituals in config

The three taxonomies have three different rates of change, so they are stored three
different ways.

**Scenes are an enum.** Twenty-two variants, exactly section 2.1's list. They are a
`match` arm in every consuming phase, a column in `scene_profiles`, and the output arity
of a trained model. Adding one is a model retrain, a migration and a profile row - it is
not a config edit, and pretending otherwise would let a photographer add a scene the
classifier can never emit.

**Chapters are an enum.** Eight from the phase card's ordered story - Getting Ready,
Details, Ceremony, Rituals, Portraits, Reception, Dance, Exit - plus `Other`, which
section 6.4 requires: "unknown or genuinely novel events map to `other` with a description
rather than a wrong confident label". The HMM transition matrix is indexed by this enum,
which is why it is closed.

**Rituals are config.** `crates/aura-brain-wedding/config/rituals/{hindu,nepali,christian,
muslim,civil}.toml`. Section 2.1 calls the taxonomy extensible and section 12's first
failure mode is cultural blind spots; a tradition that needs a rite this product has never
heard of must be addable by editing a file, not by shipping a build.

`RitualId` is therefore a `u16` **authored in the TOML file**, not derived from file order.
Deriving it from order would mean that inserting a rite renumbered every rite after it and
silently relabelled every stored row. A test asserts that ids are unique across all
loaded files and that no id is reused. The catalog stores the **slug**, not the id, so a
catalog stays readable when a taxonomy file is edited, and `image_scenes.taxonomy_ver`
records which taxonomy produced it.

### 4. The scene vocabulary has three spellings, and the mapping is code

Three vocabularies already exist in this repository and they do not match:

| Where | Size | Written by |
|---|---|---|
| `SceneId` | 22 | this phase, section 2.1 |
| `aura_cloud::tasks::ALLOWED_SCENES` | 18 | phase 04, frozen into three cassettes and every cached answer |
| `aura_vision::face::roles::SCENE_*` | 4 | phase 06, the couple contest's scene terms |

They are not reconciled by renaming anything. `SceneId::cloud_label()` maps the 22 onto
the 18, and `SceneId::role_label()` maps them onto the 4. Both are total, both are `const
fn`, and both are tested exhaustively against the other crates' constants.

The alternative - editing `ALLOWED_SCENES` to match - would bump `SegmentNaming::VERSION`,
invalidate three recorded cassettes and every cached cloud answer in every installed
build, and buy nothing: the coarse label is what a model can actually judge from a contact
sheet, and `getting_ready_bride` versus `getting_ready_groom` is a question about who is
in the frame, which is `PeopleService`'s question and not a vision model's.

### 5. `SegmentNaming` keeps phase 04's field names

Section 7 of the phase document specifies a response schema whose keys are `chapter` and
`split_at_index`. Phase 04 already shipped `SegmentNaming` with `scene` and
`boundary_hint`, and froze it at `VERSION = 1`.

**They are the same two fields.** Renaming them would bump the task version, which the
phase 04 rule requires - "bump `CloudTask::VERSION` on any prompt, schema or ceiling
change" - and that bump invalidates `tests/cloud/cassettes/`, every cached answer, and the
audit rows that reference the old version. The gain would be that two documents use the
same noun.

So the task is unchanged and `aura-brain-wedding` owns the translation: a
`SegmentNamingOutput.scene` is parsed back through `SceneId::from_cloud_label`, and
`boundary_hint` is parsed as a tile index. Section 7's other constraint - **at most 16
calls per wedding**, against phase 04's `IMAGES_PER_CALL = 40` - is a caller-side policy
and lives in `aura_brain_wedding::story::naming::MAX_CALLS_PER_WEDDING`, asserted before
the gateway is reached.

### 6. Segmentation is PELT over a fused signal, and the HMM runs after it

Section 6.2 asks for change-point detection over embedding distance and HMM smoothing over
the timeline. The order matters and the phase document's diagram draws them in parallel,
so it is settled here: **the HMM smooths per-frame scene posteriors first, PELT segments
the smoothed timeline second.**

Smoothing after segmentation would let one misclassified frame create a boundary that no
amount of later smoothing removes - the boundary is already in the segment table. Section
6.2's own sentence agrees: "this single trick removes most absurd labels", and a label
that has already become a chapter is not a label any more.

The three hard rules from section 6.2 are constants, not tuning:

* a time gap above `HARD_GAP_MS` (20 minutes) is a boundary regardless of the signal;
* a segment below `MIN_SEGMENT_MS` (90 s) **or** `MIN_SEGMENT_FRAMES` (8) merges into its
  nearest neighbour unless its dominant posterior differs by more than
  `DISTINCT_POSTERIOR_MARGIN`;
* the PELT penalty is searched, not fixed, so that the chapter count lands in
  `CHAPTER_BAND` (6 to 20). A fixed penalty tuned on a 10-hour wedding produces two
  chapters for a 3-hour registry office and forty for a three-day Nepali wedding.

### 7. Every threshold this phase publishes is a number with an owner

`crates/aura-brain-wedding/config/scene_profiles.toml` is versioned, shipped, and
overridable per project. Section 12's third failure mode is that it becomes a dumping
ground of magic numbers, so the file's schema **requires a `rationale` string on every
scene**, and `SceneProfileRegistry::load` refuses a profile without one -
`AURA-ML-5024`. A value nobody can explain does not load.

## Consequences

* One new crate, `aura-brain-wedding`, and one new frozen file in `aura-core`. Both are
  re-locked in `contracts.lock`.
* Migration 7 adds `image_scenes`, `segments`, `segment_images` and `scene_profiles`.
  Schema version becomes 7.
* Phase 06's condition C3 half-closes: `aura_people::People::regroup` now reads
  `image_scenes` and passes real coarse labels into the co-occurrence graph, so
  `RoleOutcome::scene_starved` becomes false and `SCENELESS_CONFIDENCE_CEILING` stops
  capping the couple decision. The other half - an audit on twenty real weddings - stays
  open, because it needs weddings.
* Two placeholder models are signed into `models.lock`. As with phases 05 and 06 they
  carry no trained semantics; the gates in section 10.1 are measured against synthetic
  ground truth whose answer is known by construction. This is condition C1 of the phase 07
  exit report and it is a Sev 2 trigger.
* `SceneId::role_label` and `SceneId::cloud_label` are now load-bearing in two other
  crates. A new scene added without extending both is a compile error, because both are
  exhaustive `match` expressions with no wildcard arm.

## Alternatives considered

**A flat 22-way scene with no chapters.** Rejected: the product's promise is an ordered
story, and a chapter strip built by grouping adjacent identical scene labels fragments the
moment a single frame is misread. The chapter is a different object with its own
confidence, its own key frame and its own user lock.

**Chapters as config, like rituals.** Rejected: the HMM transition matrix is indexed by
chapter, and a transition matrix whose dimensions are read from a file at runtime is a
matrix that can be the wrong size. Rituals have no such coupling.

**One vocabulary, reconciled across all three crates.** Rejected in decision 4; the cost
is three dead cassettes and every cached cloud answer, and the benefit is cosmetic.

**Storing the ritual id rather than the slug.** Rejected in decision 3: a catalog whose
meaning depends on a config file that has since been edited is a catalog that lies.
