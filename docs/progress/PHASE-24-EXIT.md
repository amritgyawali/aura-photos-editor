# Phase 24 exit report - Generative Cleanup & Distraction Removal

**Status: implemented conditionally.** Five conditions, two of them Sev 2. `aura-cli verify --phase
24` exits 0.

---

## 1. The one-sentence version

The safety engine, the source ordering, the self-check, the store, the disclosure and the whole
command surface are built, tested and measured. **This build proposes no removals on a real
photograph, and that is the correct behaviour rather than a gap** - there is no trained detector to
name what it finds, and no mask coverage to prove a region is clear of people. Both are visible in
the outline rather than hidden, and both are conditions below.

---

## 2. Acceptance criteria, section 13

| Criterion | Status | Evidence |
|---|---|---|
| Common wedding distractions are detected and proposed for removal with previews | **Conditional** | `detect::candidates` finds unexplained salience and names nothing; `BeforeAfter` renders the preview. No trained detector, so nothing is *classified*. C2. |
| Nothing overlapping people, dresses, rings or cake can ever be auto-removed | **Yes** | `gate_1` sweeps six kinds over a 10x10 grid; the phase gate makes 300 adversarial attempts with zero successes; migration 24 refuses the two forbidden classes with a CHECK and a trigger |
| Real pixels from sibling frames are preferred over generated pixels | **Yes** | `gate_9` measures it on two backgrounds; `CleanupMethod::preference` puts the ordering in the type and nothing configurable reorders it |
| Failed inpaints revert themselves before the user ever sees it | **Yes** | `gate_10b` paints all three artefacts and each is caught; `queue::plan` runs the self-check inside the same call that produces the pixels |
| Every cleanup is disclosed in the recipe and the delivery report | **Yes** | `Recipe.cleanup[]` plus `cleanup_disclosure`; a trigger aborts `applied = 1` with no disclosure, a second aborts every UPDATE on one, a third refuses to delete one while the removal stands |
| An adversarial audit cannot make the system damage a photograph | **Conditional** | 300 mechanical attempts, zero successes. **The human audit of section 9's QAIQ row did not happen.** C4. |

---

## 3. The conditions

### C1 - No mask coverage reaches this pass, so every candidate is refused. **Sev 2.**

`AppState::cleanup_pass` attaches an empty coverage map, so `Coverage::Absent` blocks every
candidate at the denylist check with `CleanupCode::ProtectionUnknown` and `AURA-ML-5122`.

Two independent causes, and the second is the one that survives:

1. Phase 18's segmenter is a placeholder, so there is nothing to intersect.
2. **Phase 18's twenty mask classes contain no word for a ring or a cake.** `Protected::ALL` names
   six kinds; three map exactly, `Hands` maps onto `Skin` (a superset, which can only refuse more
   than asked), and `Rings` and `Cake` map onto nothing. A coverage assembled from phase 18 is
   therefore never *complete*, even with a trained model behind it, and
   `api::coverage_from_masks` returns `Coverage::partial` accordingly.

The second was found while building this phase and is recorded in `docs/runbooks/AURA-ML-5122.md`.
Treating an unaskable kind as clear would be the same mistake this whole phase is built around, made
one level up and much harder to see: the product would claim a region is free of the rings on the
strength of never having looked for them.

**Closes when** phase 18 gains a `Rings` class and a `Cake` class, and a trained segmenter is wired
into `CleanupPass::with_masks`. Until then `CleanupOutline::mask_covered` is 0 % on every project,
which is the honest figure. No later phase may claim a cleanup quality result while it is zero.

### C2 - There is no trained distraction detector. **Sev 2.**

`DISTRACTION_HEAD_TRAINED` is false. Section 9's DATA row asks for a labelled wedding-distraction
vocabulary on 10,000 frames; there are no wedding photographs here, so there are no labels.

What ships is `detect::candidates`, which measures unexplained salience - the half of section 6.1
that can be built from measurement. It returns `DistractionClass::Unclassified` for everything,
which `story_safe` refuses, so nothing it finds reaches a proposal.

**This is deliberate rather than a stub.** A measurement that guessed a class would be a measurement
whose output the delivery report records as a fact.

**Closes when** the labelled vocabulary exists and `ml/models/generative/train_distraction.py` runs
- the script audits the dataset before it will train, and refuses one with too few negatives, on the
argument that a class whose examples are all distractions teaches a detector that a bin is always
clutter.

### C3 - No diffusion inpainting tier exists, and nothing stands in for it.

`INPAINT_PACK_INSTALLED` is false and `inpaint::solve` returns `InpaintUnavailable` on every call.
There is no model in `models.lock`, the phase 03 interpreter has no `Resize` and no `ConvTranspose`,
and TLS is waived so no public image provider is reachable.

**There is no fallback under it, on purpose.** What would stand in is the classical fill, which the
source selector already tried and rejected by the time it reaches the inpaint arm - so a fallback
would be the product doing the thing it had just decided was insufficient and then writing
`method = inpaint` on the row. `CleanupMethod::Inpaint` in a stored disclosure means a diffusion
model ran, and there is no build in which it means something else.

Not a Sev 2: the tier is the last resort in an ordering whose first two members work.

### C4 - The human adversarial audit did not happen. **Sev 2 for the headline KPI.**

Section 9's QAIQ row asks for 300 attempts to induce damage, looked at by a person, with every
success a release blocker. What exists is 300 **mechanical** attempts in the phase gate, all of which
the safety engine refused.

The gap is the same shape as phase 21's missing naturalness audit and phase 22's missing expert
preference study: the arithmetic is measured and the human judgement is not. Section 0's headline KPI
- artefact-free rate ≥ 98 % on approved removals - is measured on painted fixtures in `gate_10`,
which proves the self-check and says nothing about a wedding photograph.

**Closes when** a QA engineer runs 300 attempts against real weddings with a real detector, which
needs C1 and C2 closed first.

### C5 - No view mounts the three panels.

`ProposalQueue`, `BeforeAfter` and `ManualRemove` exist, typecheck and are props-driven. Nothing in
`ui/src/App.tsx` renders them, exactly as with every develop panel since phase 12.

This is the standing UI-shell gap rather than this phase's, and it is the remaining half of phase
21's condition C6. The nine commands *are* registered: the three-way count is 189 handlers, 189
`#[tauri::command]` definitions and 189 typed client wrappers.

---

## 4. What was measured, and what each number means

| Measurement | Result | What it proves |
|---|---|---|
| Adversarial sweep (gate 11 and the phase gate) | 300 attempts, 0 successes | The five checks cannot be bypassed through the engine |
| Protected-overlap sweep (gate 1) | 600 positions x 6 kinds, 0 allowed | Nothing overlapping a face, hand, dress, ring or cake is ever allowed |
| Artefact-free rate (gate 10) | 100 % over 48 removals on 3 backgrounds | The self-check does not let a painted artefact through |
| Deliberate artefacts (gate 10b) | 3 of 3 caught | Each of the three failure shapes is detected |
| Clean frames (gate 10c) | 3 of 3 passed | The check is not simply refusing everything |
| Sibling preference (gate 9) | Borrow chosen whenever a clean sibling exists | Section 6.3's "real pixels first" |
| Policy table | 23 scene rows, none relaxes a bound | The file can only make the product stricter |
| Refusal ratio | 16 of 31 codes | The highest proportion in the product |
| Storage | 2,763 B/image measured, 3,200 B budgeted | A list rather than a verdict, and 40 % of it is the refusals and their index |

**Every one of these is measured against frames whose object was painted in at a rectangle the test
already knew.** That is C2, and it is printed at the end of every gate run rather than left in a
document.

---

## 5. Performance

Section 11's five budgets are written about full-resolution operations on GPU hardware this build
does not link. They are recorded in `perf/budgets.toml` against the **proxy** the decision path
actually runs on, with the substitution named in the file.

| Section 11 row | Budget | What is measured here |
|---|---|---|
| Classical fill per region (45 MP) | ≤ 400 ms | Proxy region, ≤ 200 ms |
| Sibling borrow per region | ≤ 700 ms | Proxy region, ≤ 350 ms |
| Diffusion inpaint per region | ≤ 3 s | **Waived**: no tier exists (C3) |
| Detection per image | ≤ 45 ms | ≤ 6 ms - this pass opens no pixels for detection |
| Cleanup share of a 1,000-image export | ≤ 12 min | **Waived**: nothing is applied on this build |

Two waivers, both consequences of conditions above rather than of performance work not done.

---

## 6. Rollback

- **Feature flag:** `CleanupPass::with_enabled(false)`. A disabled pass still examines every
  photograph and writes a row saying so, with no proposals - because a photograph nobody looked at
  and one with nothing to tidy are delivered identically.
- **Migration:** the `DROP` sequence is in the header of `0024_cleanup.sql`. It is recomputable with
  two exceptions, both of them a photographer's own decisions: rows with `accepted IS NOT NULL` and
  images with `disabled_by_user = 1`.
- **The one rollback in the product that can change a delivered photograph.** Dropping
  `cleanup_disclosure` removes the record of a removal that is still in the recipe. Export the
  disclosures first.
- **Model version:** none shipped, so nothing to pin.

---

## 7. What phases 27 and 28 inherit

- **`CleanupService` is the only way to ask what was removed from a photograph.** Twentieth service
  of its kind. Phase 27 has to be able to say why a background looks smeared; phase 28 must know
  what ran unattended. Neither keeps its own detector, denylist or idea of a safe removal.
- **The safety filter runs before the score, and the score cannot see it.** `source::select` takes a
  `SafeCandidate`, which has no public constructor. A later phase that finds itself scoring an
  unchecked region cannot obtain the argument.
- **A cloud call that can only make the product do less has no unsafe failure mode.** `Answer` has no
  approving variant, and the offline fallback is identical to the most cautious answer. That is the
  property phase 12's tie-breaker lacked, and it is why the same repository reaches opposite
  conclusions about two superficially similar features.
- **A disclosure is written in the same transaction as the removal and can never be edited.** Three
  triggers. A removal that is applied has a disclosure; a disclosure that exists is true.
- **Nothing in this build applies a removal unattended.** Section 6.4 permits a tier-one removal at
  calibrated 0.97 in Zero-Touch; phase 13's `uncalibrated_raises` moves every band one further
  toward review while nothing is calibrated, so `may_apply_unattended` is false everywhere. That is
  the composition of two rules, neither written for this phase.

---

## 8. Sign-off

| Role | What they signed | Status |
|---|---|---|
| CTO | `docs/generative-policy.md`, and the rule that AURA never adds wedding content | Signed |
| PM + SEC | `cleanup_policy.toml`, jointly - the only file in the product with two owners | Signed |
| MLL | The detector and artefact-classifier design, and the decision to ship measurement | Signed, with C2 and C4 recorded |
| SEC | The adversarial review: the denylist cannot be bypassed through any path in the crate | Signed, mechanically. The human audit is C4 |
| PERF | The two waivers in section 5 | Signed |
| QAIQ | - | **Not signed. C4 is theirs.** |
