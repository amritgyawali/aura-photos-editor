# ADR-0055 - Quality control: quantified findings, a bounded re-edit loop, and a planner that cannot execute

**Status:** accepted · **Date:** 2026-08-31 · **Phase:** 27 · **Supersedes:** nothing

Phase 27 section 4 names no ADR. It needs two, and this is the first.
[ADR-0056](ADR-0056-qc-ipc-surface.md) covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned to
phase numbers.

## 1. Context

Twenty-six phases decided things. This one decides whether those decisions were any good, which is
a different kind of problem in three ways that shape every choice below.

**It is the first phase whose subject is the product's own output.** Every earlier phase reads a
photograph and writes a judgement. This one reads *judgements* - phase 15's illuminant, phase 16's
grade, phase 20's texture report, phase 25's node target - and decides whether they cohere. The
input is the catalog rather than the sensor.

**It is the first phase permitted to undo another phase's work.** `Remedy::RevertOp` and
`Remedy::ReduceStrength` change what a delivered photograph looks like, and
`Remedy::ReplaceFrame` changes which photograph is delivered at all. Nothing before this has had
that authority, and the failure mode is specific: a QC agent that is wrong makes a gallery worse
than the gallery it inspected, silently, one frame at a time.

**Its headline number is a claim about the product rather than about a wedding.** "Catches >= 90 %
of injected defects" is measured against defects this repository authored. That is a real gate -
it fails when a check stops working - and it is not evidence that a photographer would agree with
a ticket. Section 10.1's agreement study is the number that would be, and it does not exist.

Section 5 freezes `QcTicket`, `Remedy` and `QcReport`, and leaves eleven things undefined that the
implementation cannot proceed without: what `Evidence` is here, what an autonomy band means for a
remedy, what "measurably improve" is measured against, what happens to a ticket a photographer
disagrees with, what a check does when its input is absent, how a replacement is proven not to
break coverage, and what the planner is allowed to know.

## 2. Decision: a ticket has its own id, and it outlives its subject

`TicketId` is the sixteenth typed id and the first that names a **problem**. The alternative -
keying a finding on `(image_id, category)` - would have needed no new id and was rejected for two
reasons.

One photograph can carry two findings in one category. Two faces in a family formal can each be
off in skin, at different magnitudes, with different remedies; collapsing them loses the second.

And section 6.3 attaches rounds to a ticket. A second round on the same `(image, category)` under
the collapsed key would either overwrite the first round's record or be indistinguishable from it,
and "the product tried twice and gave up" versus "the product tried once and it worked" is exactly
the distinction the loop bound exists to make. A bound whose evidence is unreadable is not a bound.

A ticket also outlives what it is about, which is unusual in this schema. A frame replaced by its
runner-up leaves a ticket that must keep pointing at the replacement it caused, and a reverted
remedy leaves a ticket whose entire value is the record that something was tried and put back.

## 3. Decision: the diagnosis is rendered, never stored

Section 5's `QcTicket::diagnosis` is a sentence - `"bride face 4.2 dE00 magenta vs node anchors
#817/#819/#825"`. It is on the frozen struct and it is **not a column**. `QcStore` persists the
code, the deviation, the threshold and the evidence, and `QcTicket::render_diagnosis` builds the
sentence from them on read.

This is phase 09's rule, which cost that phase a schema revision to learn: reasons store their code
rather than their sentence, because a stored sentence is copy a release can change and a catalog
full of English cannot be translated. It matters more here than it did there. A QC ticket is the
single most user-facing sentence the product produces - it is the artefact section 1 says
photographers want most - so it is the sentence most likely to be rewritten between releases, and a
studio that keeps QC reports for its records would end up with two weddings whose identical findings
read differently because of a copy change.

The cost is that the numbers in the sentence must be reconstructable, which is why `deviation`,
`threshold`, `evidence` and the reason codes are all on the row.

## 4. Decision: improvement is measured against what was wanted, not against what was agreed

Section 6.3 says a remedy is kept when the metric "improves by at least the expected gain margin".
There are two candidate baselines and only one of them works.

The wrong one is to re-run the check and compare against the *threshold*: a ticket whose deviation
was 4.2 against a threshold of 2.5, remediated to 3.9, has improved and still fails, and a rule
that keeps only what passes throws away every partial repair on the hardest frames - which are the
frames a photographer most wants helped.

The right one is to compare against the deviation the ticket was **opened with**, and to require
that the realised gain is at least `MIN_GAIN_SHARE` of the gain that was predicted. A remedy that
promised 2.0 dE00 and delivered 0.1 did not work, whatever the absolute number says.

This is phase 19's lesson in a new place. That phase asked "was this lift capped" by comparing
against a target that had already absorbed the caps in order to be reachable, so nothing was ever
reported as capped and every unit test passed. **A converged value cannot be used to detect its own
constraints.** Here the equivalent mistake is comparing a remediated frame against a threshold the
remediation was allowed to move toward.

`MIN_GAIN_SHARE` is 0.50 rather than 1.0 for the reason phase 25 lowered its own reduction gate:
a remedy that realises half of an honest prediction is a remedy that helped, and a build that
reverted it would spend two rounds achieving nothing on every frame whose predictor is imperfect -
which, with placeholder heads underneath, is all of them.

## 5. Decision: collateral damage is checked on the checks a remedy can reach, and the list is in code

Section 6.3 requires that "no remedy may worsen another check by more than a small tolerance
(checked by re-running affected checks)". *Affected* is doing the work in that sentence and section
6 does not define it, so this ADR does.

Re-running all ten checks after every remedy is the obvious reading and it is not affordable: the
budget is 90 s per thousand images for the whole pass, and ten checks per remedy per round over two
rounds is the pass run five times.

`Remedy::collateral_checks` is a `const fn` on the frozen enum returning the categories that remedy
can move. It is a property of the remedy rather than a configuration, because it is a fact about
what the operation touches: reducing retouch strength cannot change a crop, and re-solving white
balance can change consistency, skin and exposure and nothing else. Putting it in a TOML file would
make it a thing a studio could get wrong, and getting it wrong is invisible - the pass would still
run, still report, and simply stop noticing one class of damage.

`MAX_COLLATERAL` is 0.10 of the affected check's own threshold rather than an absolute number,
because the ten checks are measured in five different units and one tolerance cannot be stated in
dE00 and stops at the same time.

## 6. Decision: a replacement is refused by a filter, never scored against one

Phase 12 wrote the rule for coverage guarantees, phase 23 for crop safety and phase 24 made it
structural with a type that has no public constructor. This phase inherits all three.

`replace::consider` re-validates coverage **before** the replacement's metrics are compared, and a
candidate that would leave a must-have uncovered is not a worse candidate - it is not a candidate.
The temptation this avoids is precise and would look reasonable in review: score the swap, notice
it breaks coverage, and dock it. A docked swap wins as soon as its metrics are good enough, and
what "good enough" means is a tuning parameter.

Replacements additionally require `REPLACE_CONFIDENCE_FLOOR` = 0.85 against 0.60 for a parameter
fix, because section 6.4 asks for it and because the two mistakes are not comparable. A parameter
fix that is wrong produces a photograph that is slightly worse and is reverted by the next round; a
replacement that is wrong delivers a *different photograph*, and a photographer looking at a
gallery has no way to know a frame they never saw was the one they would have chosen.

## 7. Decision: the planner cannot execute, and the property is in the type rather than in the caller

Section 6.2 says the planner "never executes anything; it proposes remedies which the mechanical
engine validates against policy before applying". Phase 24 made the equivalent guarantee structural
and this phase copies the mechanism rather than the promise.

`planner::Plan` carries `Vec<ProposedStep>`, and a `ProposedStep` is not a `Remedy`. It holds a
remedy *kind*, a target string and a magnitude, and the only route from one to the other is
`remedy::validate`, which takes the current ticket, the current recipe and the policy, and returns
`Option<Remedy>`. A step naming an operation the policy does not permit, a magnitude outside the
contract's bounds, or a frame that is not this ticket's frame, produces `None`.

So the failure modes are all the same: an unreachable provider, a spent budget, a malformed
response, a hallucinated parameter name and a plan proposing something forbidden all leave the
image with its mechanical triage. That is phase 24's property - a cloud call whose every failure
mode equals its most conservative answer - and it survives here because the planner's output type
cannot express an action.

The planner is additionally denied identity. It receives ticket numbers, a recipe summary, node
statistics and up to three crops, and there is no field on `QcPlanInput` an identity handle, a role
or a face count could go in. Phase 06's rule, and this task has no reason to know who is in the
frame: the question is whether a set of measurements has a common cause.

## 8. Decision: an absent input is a skipped check, and it is a different row from a passed one

Phase 24 wrote this rule for its safety engine and this phase applies it to all ten inspections.

A consistency check over a frame in a node with no target has not passed; it has not run.
`QcCode::CheckSkipped` and the per-category `*Unavailable` codes are separate from every finding
code, `QcOutline::checked` counts what actually ran, and `QcReport::skipped` is on the report a
photographer reads.

The alternative - treating an unmeasurable frame as clean - is what makes an automated QC pass
dangerous rather than useless. A wedding where phase 18's masks are absent would report zero mask
artefacts and read as a clean bill of health, and this build ships with several heads untrained, so
that is the common case here rather than the exotic one.

## 9. Decision: a photographer's disagreement is a status, and section 5's enum gains one variant

Section 5 lists five statuses: `Open | Fixed | Reverted | Escalated | Accepted`. Section 11's
telemetry lists `qc.user_disagree`, and there is nowhere in the five to put it: `Accepted` is a
photographer agreeing with a finding, and a photographer who thinks the finding is wrong needs a
different row from one who has not looked yet.

`TicketStatus::Dismissed` is the sixth variant. It is a contract amendment under this ADR - the
fifth in the product's history, after phase 09's `FaceRef`, phase 16's re-lock, phase 23's
`Lens::coefficients` and phase 24's `Recipe.cleanup[]`.

Dismissal is also the one status automation may never write. `QcStore::sweep` excludes it from
re-analysis, exactly as `user_edited` is excluded everywhere else in this schema, because a ticket
a photographer has rejected that comes back on the next pass is a product arguing with its user.

## 10. Decision: the report is generated, and the export is Markdown

Section 2.1 asks for a report "exportable as PDF/Markdown". Markdown ships; PDF does not.

A PDF writer is a dependency, a font-embedding decision and a page-layout engine, and none of the
three is a quality-control question. Markdown is text a studio can archive, diff, paste into an
email and convert with any tool they already have, and `report::to_markdown` is a pure function
over `QcReport` - which makes the report's content testable rather than its rendering.

## 11. What this phase does not build, and why

**Learning from resolutions.** Section 2.2 puts it in phase 30. The outcome columns this phase
writes are the input that phase consumes; nothing here fits anything.

**A defect-detection model.** Section 9's DATA row asks for a labelled corpus of real defective
weddings and there is none. Every check here is a *measurement* against another phase's stored
number, which is the same argument phase 21 made for its glare and lint detectors: a measurement's
failure mode is finding fewer problems rather than confidently inventing them.

**Any new pixel operator.** The remedies re-run phases 15 to 26; they do not add an eleventh way to
change a photograph. `crates/aura-qc/tests/no_pixel_ops.rs` is the seventh grep-as-a-test in the
repository and fails the build if this crate writes a recipe, reaches a provider outside
`aura-cloud`, or grows an operator of its own.

## 12. Consequences

QC findings are numbers with thresholds, so a ticket is testable and a report is auditable. The
loop cannot thrash, because every round must realise half of a predicted gain against the deviation
the ticket opened with, and two rounds is a hard bound. A replacement cannot break coverage,
because coverage is a filter rather than a term. The planner cannot act, because its output type
has no executable variant.

The cost is that this phase is only as good as the numbers underneath it, and in this build most of
those numbers come from placeholder heads. What the gates prove is that the inspections fire on
defects this repository injected and stay quiet on frames it did not. That is condition C1 of the
exit report, and it closes with phase 05's C10 rather than separately.
