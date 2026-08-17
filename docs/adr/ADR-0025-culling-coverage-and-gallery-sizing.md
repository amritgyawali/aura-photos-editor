# ADR-0025 - The culling engine, the story coverage guard and gallery sizing

**Status:** accepted  
**Date:** 2026-08-16  
**Phase:** 12 - Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing  
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Every phase from 05 to 11 has ended with the same sentence in a different form: *a
measurement is evidence, and the deciding phase owns the decision.* This is the deciding
phase. Phase 12 is the first place in AURA where a number turns into an absence: a frame
that is not selected is a frame the couple never sees.

That changes what the contract has to protect. The previous seven phases protected
themselves against being *misread* as verdicts. This one has to protect the wedding
against being *correctly* read and still wrong - because a selection that is defensible
on every individual frame can still lose the only photograph of the ring exchange, and
section 1 of the phase document is blunt about what that costs: "a single lost 'must-have'
frame loses a customer forever."

So the engine is not a threshold. It is a constrained optimisation whose hard constraints
run **last**, after every soft preference has already had its say, and whose output
carries the reason for every frame in both directions.

## 2. Ten contract spellings differ from section 5

`crates/aura-core/src/contract/cull.rs` is the frozen contract. These differences from the
phase document are intentional.

| # | Section 5 | Shipped | Reason |
|---|---|---|---|
| 1 | `ImageId` | `PhotoId`, aliased | Eighth contract, one identifier. A second image id is a second join key. |
| 2 | `Reason` | `CullReason` with a typed `CullCode` | `Reason` is already taken by the phase 09 integrity contract in the same crate, and a free string cannot be translated, counted or asserted against. |
| 3 | `Selected::moment_id: MomentId` | `Option<MomentId>` | Phase 08's coverage denominator is *groupable* frames. A frame with no embedding is in no moment, is still deliverable, and must not be represented by a fabricated moment id. |
| 4 | `MustHave` referenced, not defined | frozen 12-variant enum | Section 2.1 names eleven must-haves and section 6.3 adds the per-identity rule. A vocabulary the config file indexes has to be closed, or a typo in TOML silently disables a guarantee. |
| 5 | `Coverage` referenced, not defined | `Covered` / `CoveredWeak` / `Missing` | Section 6.3's three states exactly. `Missing` means *no candidate existed*, never *we chose not to*. |
| 6 | `CullMode` referenced, not defined | frozen 3-variant enum | Section 2.1's three autonomy modes. `Zero-Touch` is phase 28 and is deliberately not a fourth variant here. |
| 7 | no rejection shape | `Rejected` | Section 2.1 requires "rejection reasons for every rejected frame"; a shape with no `kept_instead` would make the panel guess what won. |
| 8 | no coverage carrier | `CullOutline` | Phase 05's rule, eighth time: report coverage and say what the denominator is. |
| 9 | no entry point | `CullService` | Phases 13, 14, 27, 28, 29 and 30 all consume the selection. Six private paths into `selection` is six answers to "what is being delivered". |
| 10 | `deterministic_hash: u64` | kept, and defined | The hash is over the *inputs and the config*, not over the output, so it identifies the question rather than the answer. Section 6.4 and the support case it is for. |

## 3. Decision: fusion is multiplicative, and two things bypass it

`keep_score` is a scene-weighted geometric mean of four sub-scores in log space:

```
keep_score = exp( (w_t·ln t + w_e·ln e + w_c·ln c + w_p·ln p) / (w_t + w_e + w_c + w_p) )
```

Section 6.1 requires that "a catastrophic technical failure cannot be rescued by emotion
and vice versa", and a geometric mean is the only common fusion with that property: one
factor near zero drags the product to zero regardless of the other three. A weighted sum
does not, and a weighted sum is what every competitor ships.

Two classes of frame never reach that arithmetic.

**Hard vetoes** (section 6.1) reject before fusion, with a reason that names the physical
fact rather than the score: the subject is out of focus, exposure is `lost`, or the
primary identity's eyes are closed without intent in a posed scene. A veto is a
*measurement*, not a threshold on a composite, which is why it can be explained in one
sentence to a photographer who disagrees.

**Hard promotions** (section 6.1) protect a frame the coverage guard needs even when its
score is low, and the reason says so honestly: "the only frame of the ring exchange".

The aesthetic contribution is bounded below technical integrity and emotion in every scene
row, because ADR-0023 section 3 already decided that taste breaks ties rather than
overriding substance and phase 12 is where that decision is spent.

**Per-scene calibration ships as the identity map.** Fitting isotonic regression needs
labelled keeper/reject pairs from real weddings, and there are none in this repository.
`calibration_ver` is `0` for the identity map so that a fitted table can never be confused
with an unfitted one. This is condition C2 in the exit report.

## 4. Decision: the coverage guard runs last, and it cannot be turned off

The pass order is fusion → moment → chapter → **coverage** → diversity → sizing → coverage
again. The guard appears twice on purpose:

* after the chapter pass, so a quota cannot starve a must-have;
* after sizing, so shrinking the gallery cannot break one - section 6.4's "the coverage
  guard always runs last so shrinking never breaks must-haves".

`CullMode::Aggressive` shifts thresholds and k-values. It does not touch a single field
the guard reads, and `modes.rs` has no access to the rule table at all: the type system
carries that guarantee rather than a review comment. Section 10.1's "Aggressive mode still
satisfies all coverage rules" is therefore a property, not a test result that could drift.

When a rule cannot be satisfied because the photographer never shot it, the report says
`Missing` and the warning says "no candidates found". **The product does not invent
coverage and does not hide the gap.**

## 5. Decision: identity coverage is read from the subject hierarchy

Section 6.3 requires "every close-family identity gets >= 3 frames, every recurring guest
>= 1". `PeopleService` exposes `SubjectHierarchy` - `primary`, `secondary` and a weight per
identity - and does not expose a per-identity `Role`. Rather than amend a frozen phase 06
contract, phase 12 reads:

* `primary` as the couple,
* `secondary` as close family and anybody the photographer marked important,
* `weights` as the recurrence signal for the guest rule.

`SubjectHierarchy::couple_unconfirmed` is surfaced as a warning on the coverage report,
because section 6.3's whole point is that this feature prevents "my aunt isn't in the
gallery" and an unconfirmed couple means the *primary* identities may be wrong too.

## 6. Decision: gallery size is a bounded linear model, and it is shipped unfitted

Section 6.4 asks for a regression over shoot volume, moments, chapters, hours and the
keeper-score distribution, trained on sixty real delivered galleries. There are none here.
What ships is the same feature vector with **authored** coefficients whose output is
clamped into section 6.4's stated band of 22-38 % of shot volume, and `sizing.rs` names
every coefficient with the argument for its sign.

That is honest and it is weaker than the phase asks for. It is condition C3 in the exit
report, alongside phase 10's identical situation with the Bradley-Terry ranker.

The slider is not the regression. Moving it re-runs the allocation passes over already
computed scores, which is why section 11 budgets two seconds for it and 1.5 seconds for
the passes over 4,000 frames.

## 7. Decision: three version columns, and the fourth is the config digest

`model_ver` invalidates every sub-score that came from a head. `analysis_ver` invalidates
the passes. `calibration_ver` invalidates the fused score. The fourth thing that
invalidates a selection is not a version at all - it is the **content of two TOML files** -
and a config edit that did not bump a version is exactly the support case section 6.4's
determinism hash is for.

So `SelectionResult::deterministic_hash` is computed over the candidate inputs *and* the
digest of both config tables *and* the mode *and* the target. `AURA-ML-5048` is raised when
a stored selection's hash does not match a freshly computed one. Eighth phase, eighth
version-drift code.

## 8. Decision: an override is unbeatable and is re-applied, not excluded

Sixth phase running. `selection.user_action` records `keep` or `reject`, it is checked
*inside* the statement a re-selection would overwrite the row with, and a re-run
**re-applies** it to the freshly computed selection rather than subtracting the frame from
the input. Phase 09 wrote the reason: a dismissed flag does not replace the measurement,
so the frame is still re-measured and the disagreement is carried onto the new one.

A forced keep counts toward the chapter quota and toward coverage. A forced reject
**cannot** break a must-have: if the frame the photographer rejected was the only candidate
for a rule, the rule degrades to `Missing` with a warning that names the override. The
product does not silently overrule the photographer and does not silently lose the vows.

## 9. Consequences

* Six later phases consume `CullService`. None of them re-derives a keeper.
* The engine is pure: `select()` takes plain data and returns a `SelectionResult`, with no
  database, clock or network in reach. That is what makes byte-identical output across two
  machines a unit test rather than a field report.
* Nothing in this phase deletes a file, moves a file or writes to a RAW. A rejection is a
  row.
* The three placeholders - calibration, the size regression, and every sub-score that
  comes from a phase 06/09/10/11 placeholder head - mean **no number in this phase is a
  claim about a real wedding's pixels yet.** That is condition C1, it is a Sev 2 trigger,
  and it closes with phase 05's C10 rather than separately.
