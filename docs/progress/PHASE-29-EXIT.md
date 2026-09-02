# Phase 29 exit report - Curation Intelligence

**Status:** implemented conditionally. Five conditions, three of them Sev 2.

Phase 30 may start. Nothing in it may claim an agreement result, a reordering rate or a monochrome
acceptance rate until C4 closes, and **nothing anywhere in the product may describe a monochrome
suggestion as protecting somebody's skin** while C2 stands — the rule is real, and on this build it
has nothing to apply to.

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contract | `crates/aura-core/src/contract/curate.rs`, `SpreadId` in `contract/ids.rs` |
| Decisions | `docs/adr/ADR-0059-curation-selection-and-album-composition.md`, `ADR-0060-curate-ipc-surface.md` |
| The crate | `crates/aura-curate/src/` — readings, monochrome, heroes, album, spreads, social, teaser, captions, cloud, export, store |
| Schema | `crates/aura-catalog/migrations/0029_curation.sql` — eleven tables, two views, five triggers |
| Policy | `crates/aura-curate/config/curation.toml` — album sizes, rhythm, weights and formats, each with a written reason |
| IPC | `crates/aura-app/src/curate_commands.rs` — eleven commands and the `Field` port |
| Panels | `ui/src/components/curate/` — six components, 21 tests, mounted in `App.tsx` |
| Gates | `tests/eval/curate_eval.rs` (25), `crates/aura-curate/tests/no_outputs.rs` (9), `tests/store_round_trip.rs` (12), 136 unit tests in `aura-curate`, 27 contract tests in `aura-core` |
| Study runners | `ml/models/curate/eval_curate.py`, `train_hero.py`, `train_bw.py` |
| Budgets | `crates/aura-perf/tests/curate_budgets.rs` (5), `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 29` |
| In the product's own voice | `docs/curation.md` |

**No model.** The ninth phase since 08 to ship none, and the reason is phase 17's, 23's and 25's
rather than phase 24's: there is nothing here that a model could do better than the arithmetic until
there is data. The hero blend has five terms phases 09 to 12 already measured, and the monochrome
suitability is four measurements over phase 05's stored descriptors whose failure mode is offering
*fewer* candidates rather than confidently wrong ones. `HERO_HEAD_TRAINED` and `BW_HEAD_TRAINED` are
both false and both are on the wire, so a panel cannot present a solver's answer as a learned one.

**One cloud task, and it can only be agreed with.** `AlbumSequencing` proposes moves and captions.
Every move is applied only when the deterministic objective agrees it is an improvement and refuses
outright if it crosses a chapter; every caption passes the same closed-vocabulary check the local
template passes by construction. An unreachable provider, a refused answer, a spent budget and a
cautious model all produce **the same album**. Phase 24 gave its editorial judgement this property
and this is its second application.

**One rule this phase is built on, and it is a negative.**
`crates/aura-curate/tests/no_outputs.rs` is the ninth grep-as-a-test in the repository. It fails the
build if this crate writes a recipe, opens a file, reaches a socket, opens a photograph, grows a
second similarity index, acquires a constant it could compare a person's skin against, or lets any
module other than the store name `album_order`. The manifest is the first lock — no `aura-recipe`,
no `aura-render`, no deciding crate — and the grep is the second.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Heroes, a sequenced album with paired spreads, social sets and a teaser | **Met** | The gate runs all four on both fixture shapes: 20 heroes, 69 spreads, 80 album images, 18 teaser frames, three social sets |
| 2 | B&W suggestions come with per-frame mixes, not a single preset | **Met** | `bw::solve` reads that frame's own eight bands; `gate_3` measures the separation it achieves through the real renderer, and `gate_3d` is the concrete sense in which it beats a preset |
| 3 | Album coverage of must-haves and close family is guaranteed | **Met** | `gate_4` and `gate_4b` on seven seeds and both shapes; coverage is a filter applied before any score, so it cannot be outvoted |
| 4 | Every pick is explained; reordering is instant and remembered | **Met** | `gate_1c` and the gate's reason check; `curate_set_order` records and re-composes in one command at 375 ms, and `album_order` is what makes it survive a re-curation |
| 5 | Album and social specs export cleanly to external tools | **Met** | Twelve hand-written specifications, every JSON one parsed back by a consumer in `store_round_trip.rs` and again in the gate |
| 6 | Photographer agreement studies meet the gates | **Not met** | Unmeasured. There are no consented weddings and no photographers. Condition C4 |

---

## 3. Section 10.1 gates

| Gate | Result |
|---|---|
| Hero agreement >= 0.75 on top-20 overlap | **0.750 worst of seven, 0.807 mean** — against `fixtures::planted`, not against photographers |
| Album reordered by <= 15 % in the study | **Not measured.** What is asserted instead: every pair the optimiser produces is one the rules permit |
| B&W accepted >= 70 %; mixes better than a preset | **Not measured.** What is asserted instead: 99.6 % of offered mixes separate the collapsed tones, and a preset always breaks the skin bound where the solver never does |
| Album coverage: every must-have and close-family member appears | **Met**, seven seeds, both shapes |
| No facing near-duplicates, no tonal clash beyond threshold | **Met**, seven seeds, both shapes, asserted through the composer's own predicate after the optimiser |
| Captions contain no invented names, places or claims | **Met** — all captions grounded; six kinds of invention refused, including a gendered role word |
| Offline: curation works fully without cloud | **Met** — every selector runs with no provider, and a refused answer produces the identical album |

Three of the seven rows are studies and are reported rather than asserted. See section 8 for the
three attempts at an arithmetic proxy that were made and abandoned.

---

## 4. Conditions

**C1 — every gate is measured on readings this repository authored. Sev 2.**
Section 9's DATA row asks for sixty real album sequences, hero sets and monochrome selections
collected with permission. There are none. Every wedding in every gate is `fixtures::wedding`: its
scores, its descriptors, its chapters and its similarity are numbers this repository chose, and the
"right" portfolio is a set `fixtures::planted` named in advance. That proves the ranking is stable,
the constraints bind, the coverage filter cannot be outvoted, the refusals are refusals and the
offline path is the whole product. **It is a test of the selector against a file in this
repository.** A fixture cannot disagree, and disagreement is what section 10.1's first three rows
are about. Closes with a consented archive; it also compounds with phase 05's C10, because the
uniqueness term reads the placeholder embedding.

**C2 — the skin rule is unreachable on this build. Sev 2.**
Where a person's measured skin band is known, the monochrome mix may not move it beyond
`MAX_SKIN_BAND_SHIFT` — not a little, not in a safe direction. The rule is real, it is enforced in
the contract, in the solver and in the schema, and `gate_3b` and `gate_3d` prove it on the `complete`
fixture. **On this build it never applies to anything.** Phase 06's detector finds no faces, so phase
15 measures no skin locus, so every mix in every real wedding is solved as a faceless frame.
`CurateCode::SkinLocusUnavailable` is on every such pick and `docs/curation.md` says it plainly.
Closes with phase 06's C1. Until then **no claim may be made that this build protects anybody's
skin in a monochrome conversion.**

**C3 — spread direction is unmeasurable, and it is the term album designers care about most.**
Which way two subjects face across a gutter is measured from phase 06's eye landmarks, so on this
build `facing_known` is false on essentially every spread and the term is renormalised out of the
pairing score. That is the honest behaviour — a spread nobody could check is not a spread that
passed, and the panel renders it grey rather than green — but it means the pairing score a
photographer reads is built from three terms rather than four. `gate_5c` asserts that a facing score
is never claimed where nothing was measured. Closes with phase 06's C1.

**C4 — the three headline gates of section 10.1 are unmeasured. Sev 2.**
Hero agreement at 0.75, an album reordered by under 15 %, monochrome picks accepted at 70 %. All
three need photographers looking at real weddings, and `ml/models/curate/eval_curate.py` is what runs
them the day one exists — from a real catalog, with denominators chosen so that a project nobody has
reviewed reports *nothing* rather than unanimity. **No claim about how much of a photographer's
curation work this saves may be made from this build.** The failure it would hide is the one this
phase makes easiest to hide: a proposal that is internally consistent, fully explained, and not what
anybody would have chosen.

**C5 — the cloud sequencing task has never reached a provider.**
`AlbumSequencing` is implemented, its schema validator is tested against over-long captions,
out-of-range indices and empty answers, and the gate proves a refused answer changes nothing. It has
never been called. Its contact sheets need a renderer this crate must not have (see the manifest
note), and TLS is waived (ADR-0009), so what is proved is that the validator refuses and the
optimiser stands — not that a model helps. Closes with a cassette recorded against a real provider.

---

## 5. What is deliberately absent

- **No `curate_apply` anywhere on the surface.** Nothing in this phase writes a recipe, and the
  monochrome suggestion is the reason the rule needed a grep behind it: the `bw` block phase 14 froze
  is two fields, `schema::merge` is one call away, and a photographer would see a beautiful result.
  That is the product deciding a wedding is monochrome.
- **No strength, threshold or weight on the IPC surface.** A studio may tighten
  `curation.toml`; nothing on the wire can widen a bound the code owns.
- **No album page designer.** Section 2.2 puts layout out of scope and section 12 names the scope
  creep by name. What ships is a *specification* an album application reads.
- **No `Approve` in the cloud answer type.** The sequencing task's moves are proposals the local
  objective accepts or refuses; there is no shape a model could return that would make the product do
  more.
- **No second coverage engine.** `CoverageReport` is phase 12's, and the album's report is the same
  vocabulary over a different set — which is what makes "the gallery covers the ring exchange and the
  album does not" a comparison rather than two unrelated numbers.

---

## 6. Rollback

Migration 29 is additive: eleven tables, two views and five triggers, none of which any earlier
phase reads. Dropping `aura-curate` from the workspace, removing the eleven commands from the shell
and reverting `App.tsx` leaves a product identical to phase 28's, with a curated wedding's rows
inert in the catalog. No frozen contract from phases 01 to 28 was amended.

---

## 7. Regression

The full workspace suite is green, the phase 01 to 28 gates are unchanged, `cargo xtask contracts
--check` reports 78 entries locked, and the IPC parity count is 240 = 240 = 240.

Two things outside this phase moved and both are worth naming:

- **`aura-curate` gained `aura-render` and `aura-recipe` as dev-dependencies only**, so
  `tests/eval/curate_eval.rs` can measure a monochrome mix on the greys it actually produces rather
  than on its band weights. Phase 16's rule — a guarantee about a pixel is enforced on the pixel —
  with this phase's constraint on top: the library must not be able to render, because rendering is
  one call from applying. `no_outputs.rs` fails the build if `src/` ever names either crate.
- **`bw::COLLAPSE` was made public** so the gate measures against the threshold the solver uses
  rather than a copy of it.

---

## 8. Five things this phase got wrong first

**A fixture that minted random identifiers looked deterministic for the whole of this phase's
development.** `ImageId::new()` is a v7 UUID: time-ordered in its high bits and *random* in its low
ones. So two runs of the same seed produced galleries that agreed about every score and disagreed
about every identifier — and every tie-break in this crate falls back on `image_id`, as does the
fixture's own similarity. The hero agreement gate moved fifteen points between two runs of an
unchanged build, which is the worst kind of red line: one nobody can reproduce.
`the_same_seed_produces_the_same_wedding` had been passing throughout, because it compared scores
and chapters rather than identifiers. **A determinism test that does not compare the identifiers is
not a determinism test.** Invariant 4, and the ids are derived from the seed now.

**Three attempts at an arithmetic proxy for a human judgement, all wrong, all plausible.** Section
10.1 asks whether a generated mix is "rated better than a fixed preset", and no statistic can answer
it. A statistic that rewards how far a mix moves the tones is won by the preset, which moves every
band by a large fixed amount; one that rewards restraint is won by the solver, for the same reason;
one that takes the minimum gap over seven bands is a lottery over whichever pair happens to
collide. Choosing between them is choosing the answer. The same happened to the album's reordering
row: three successive *distance* bounds on the sequencer, each either loose enough to prove nothing
or tight enough to fail a correct build on the eighth wedding, because successive look-ahead pulls
compound. What both rows have now is the fact underneath the judgement — the mix separates what had
collapsed, the optimiser never produces a pair the rules forbid — and the judgement itself named as
unmeasured. **When a gate cannot be met honestly, the answer is sometimes that the gate is a study.**

**A fixture wedding whose every frame was one hue.** The monochrome gate found it: a frame with one
colour in it has *nothing to separate*, so every mix and every preset scored identically on three
quarters of the gallery and the gate could not tell a working solver from one that returned neutral.
A wedding photograph is skin, fabric, foliage and sky in one frame, which is the entire reason a
per-band mix exists. The same file had two more of these, both found by the same gate: a uniform
luminance across the whole day, which models a gallery nobody normalised and which no curation pass
ever sees because phase 25 runs first; and a similarity that was a hash of two identifiers, which
makes distinctness independent of everything else about a photograph and turned eighteen per cent of
the portfolio blend into a coin toss. Phase 25's lesson, three more times: **work out whether the
fixture, the threshold or the code is the thing that does not match reality.**

**A planted portfolio that exactly filled three chapters' quotas.** `fixtures::planted` spread its
twenty picks at a fixed stride, which follows chapter length, which put exactly
`MAX_HEROES_PER_CHAPTER` plants into each of the three longest chapters. A plant set that fills a
quota is one where a single ordinary frame winning a single round costs a plant permanently, so the
agreement gate was measuring ties rather than the selector. The first fix — an even round robin —
traded that for a spacing problem, putting three plants close enough together in the shortest chapter
to be near-duplicates of each other. **A ground truth an algorithm cannot reach is a ground truth,
not a finding.**

**A budget written before it was measured, wrong by a factor of ten and in the wrong direction.**
The store note said 2,143 B per image; it measures 211 B at 600 frames and 2,439 B at the smallest
gallery, because the album, the portfolio and the captions are capped by the contract and the gallery
is not — so the per-image figure *falls* as a wedding grows, which is the opposite of every migration
from 09 to 20. The budget is set at the small end now, and the bound is asserted as well as the
number. Phase 21 wrote this rule and phase 26 wrote its second half; this is the first time the shape
was inverted rather than merely mis-sized.

---

## 9. Inherited conditions still open

Every Sev 2 from phases 05 through 28 is still open. Three of them reach directly into this phase and
are named above as C1, C2 and C3: the placeholder embedding under the uniqueness term, the untrained
detector under the skin rule, and the same detector under the facing term. Phase 05's C10 is the
root of the first, and it closes for this phase at the same time it closes for the thirteen phases
that read the embedding.

Phase 02's condition — the first real camera file reopens its criteria whatever phase is in flight —
is still the standing one.

Phase 28's C7 — "this build writes no files" — is **half closed** by this phase and half not.
`AppRunner::availability` no longer reports `SkipCause::PhaseNotBuilt` for curation, and the stage's
arm is one call into `curate_project`, which is the shape phase 28's rule asks for. Export is still
unbuilt, so a completed run leaves a curated wedding in the catalog and nothing on disk. Phase 30
closes the other half, and phase 28's own gate now prints C7 saying so.
