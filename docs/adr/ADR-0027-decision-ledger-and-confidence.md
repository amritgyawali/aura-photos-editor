# ADR-0027 - The decision ledger, calibrated confidence and the autonomy bands

**Status:** accepted · **Date:** 2026-08-17 · **Phase:** 13 · **Supersedes:** nothing

## 1. Context

Twelve phases have written evidence into the catalog and one has written a decision. Every
one of them carries a confidence and a list of reasons, because invariant 2 says a decision
without an explanation is a bug - and that has left the product with four reason
vocabularies, four confidence scales and no way to answer the question a support case always
asks: *what did it do, and why*.

Phase 13 lands here rather than later for a reason section 8 states in one line: the model
has to be frozen before the phases that will write into it exist. Phase 14 decides edits,
phase 20 retouches, phase 27 QC tickets, phase 29 albums and phase 30 exports. If each of
them designs its own record, there will be six.

Calibration is the second half, and it is a safety mechanism rather than a nicety. The
autonomy bands that make Zero-Touch defensible - 0.98 and above acts, 0.90 to 0.98 acts only
in Zero-Touch, 0.75 to 0.90 goes in a review queue - are only meaningful if 90 % confidence
really means 90 % correct.

## 2. Decision: the frozen shapes, and the five spellings that differ

Section 5's `Reason` and `Decision` are frozen in `crates/aura-core/src/contract/ledger.rs`.
Five names differ from the phase document and each difference is a compile error avoided.

| Section 5 | Here | Why |
|---|---|---|
| `Reason` | `LedgerReason` | `Reason` is phase 09's technical reason, re-exported at the crate root. |
| `Decision` | `LedgerDecision` | `Decision` is phase 12's keep-or-drop sum, also re-exported. |
| `Source` | `DecisionSource` | `Source` is phase 07's model-or-user provenance. |
| `Reason::code: &'static str` | `code: String` | A `&'static str` cannot survive a round trip through a catalog read years later by a build whose static table has moved on. |
| - | `LedgerDecision::project` | A row that cannot say which wedding it belongs to cannot be compacted, sliced into a bundle, or cascaded when a project is deleted. |
| - | `LedgerDecision::supersedes` | Section 6.3 requires corrections to supersede rather than overwrite; the pointer has to live somewhere. |

`DecisionId` is added to the frozen `ids.rs`, as phases 06, 07 and 08 added theirs. It is the
first id in that file that names an *event* rather than a thing, which is exactly why it
needs one: `aura-cli replay <decision_id>` is a support command read down a telephone, and a
composite key of `(subject, kind, timestamp)` fails on the first re-run inside one
millisecond.

## 3. Decision: the ledger is append-only, and the database enforces it

Migration 13 carries `decisions_no_update`, a trigger that aborts every `UPDATE` on
`decisions`. A correction is a new row whose `supersedes` points backwards.

`DELETE` is deliberately **not** blocked, because compaction and the project cascade need
it, and `aura_explain::ledger::Compaction` is the only thing in the workspace that uses it.
The policy - "the newest decision per subject plus all user overrides", section 6.3 - is
expressed as filters inside the `DELETE` rather than as care in a caller.

**`supersedes` is not a foreign key**, and that follows from the trigger. Every referential
action SQLite offers is either an `UPDATE` of the column (`SET NULL`, `SET DEFAULT`), which
the trigger aborts, or a `CASCADE`, which would delete a correction because the thing it
corrected was compacted away. A backward pointer into a compacted row must simply dangle.

## 4. Decision: an adapter, not four rewritten contracts

Section 8's first step says to "refactor Phases 09-12 to emit" the unified model. What
shipped is `aura_explain::adapt`, which maps each phase's own frozen reason type into
`LedgerReason`, and the deciding phases were not changed at all.

The argument: rewriting four frozen contracts means four ADRs, four migrations touched, four
IPC surfaces and four `contracts.lock` re-locks - and it buys nothing, because the property
that has to hold is *the deciding code owns the reason*, and it already does.

The discipline section 12 asks for ("a lint forbids constructing reasons outside the
deciding module") is kept in a stronger form. `aura_explain::reason::Catalog` is **assembled
from those same four enums** - `ReasonCode::ALL`, `EmotionCode::ALL`, `CompositionCode::ALL`,
`CullCode::ALL` - so it cannot go stale, and a decision citing a code that is not in it is
refused at record time with `AURA-ML-5054`. There is no way to put a reason in the ledger
that no deciding phase can emit, which is more than a lint would have given.

## 5. Decision: confidence is two numbers, and the band is stored

`raw_confidence` is what the deciding code believed; `calibrated_confidence` is what that
belief is worth. Both are stored, always.

Storing only the calibrated number would make a re-calibration unfalsifiable - there would be
nothing left to re-map. Storing only the raw one would make the autonomy band a guess.

The **band is stored rather than recomputed on read**, because a band is what the product was
allowed to do *at the time*. A build that recomputed it from today's config would answer
"what did it do" with "what would it do now", and only one of those is a support answer.

`Explain::record` **overwrites** whatever `calibrated_confidence` and `autonomy` a caller
supplied. A deciding phase that could set its own band would be a deciding phase that could
grant itself permission to act.

## 6. Decision: one calibration model per (kind, source), shipped as the identity map

Per kind because a culling decision and an export decision are wrong in different
proportions. Per source because section 6.1 says so: a cloud model's error profile is a
schema retry and a local model's is a placeholder head.

Everything ships as the identity map at `calibration_ver = 0`, and nothing else may be
version 0. Isotonic regression by pool-adjacent-violators and temperature scaling by bounded
scan are both implemented, tested and measured by `ml/eval/calibration_report.py`; what they
have to fit on is labelled outcomes from real weddings, and there are none in this
repository. That is condition C2 of the exit report.

## 7. Decision: a third risk multiplier, and why it is on

Section 6.4 names two - irreversible actions and must-have moments - and both are here.
`irreversible` is read from `DecisionKind::is_irreversible` and **never from configuration**,
because a config file that could declare a retouch reversible would be a config file that
grants autonomy over somebody's face by editing a boolean.

This build adds a third: `uncalibrated_raises`. While no calibration is fitted, every
decision is raised one band toward review.

The argument is section 6.4's own. The bands are defensible only if 90 % means 90 % correct,
and nothing in this build has established that. Raising one band means AURA does slightly
less on its own and asks slightly more often, and the decision carries the reason code
`uncalibrated_confidence` so the photographer reads *why* rather than seeing 0.99 beside a
review request and concluding the product is broken.

It should be switched off in the same release that ships the first fitted calibration. Not
before.

## 8. Decision: the support bundle carries no identifier at all

Section 2.1 allows pixels "unless the user opts in". **The opt-in is not implemented**, and
that is deliberate: it would be the one code path in the product that could put a photograph
into a file which is then emailed, and nothing in this phase needs it. Phase 27 may need it,
and it will need an ADR.

Every id in a bundle is replaced by a handle assigned in first-appearance order -
`image_0001`, `decision_0007` - and the mapping is not stored anywhere. The structure
survives, so a support engineer can still see that three decisions were about one frame; the
wedding does not.

Three guarantees, and only one of them is a filter: no pixels (structural - `Evidence` has no
variant that can hold bytes), no names (structural - nothing in migration 13 stores one), no
keys (by construction, and scanned anyway, because "it cannot happen" is what every leak was
called beforehand).

## 9. Decision: replay is a port, not a dependency

`aura-explain` must not depend on the phases that decide. `ReplaySource` is a trait: it takes
a stored decision and returns what today's code would produce. `aura-cli` implements it over
phase 12's stored selection; phase 27 will implement its own.

The comparison lives here, so all six future decision kinds share one definition of
"identical" - and one definition of the difference that matters:

* same inputs hash, different outputs → **a determinism defect**, invariant 4;
* different inputs hash → **an upgrade**, which is supposed to decide differently.

`AURA-ML-5057` says which one it is in words rather than making somebody compare two hex
strings.

## 10. Alternatives considered

**A reason code as an enum in `aura-core`.** One closed vocabulary for the whole product.
Rejected: it would put ninety-three variants from five phases into one type that every phase
has to match on exhaustively, and the first phase to add a code would break every other
phase's build. The registry gives the same guarantee at record time without the coupling.

**Recording analysis as decisions.** Phases 09, 10 and 11 produce a verdict per photograph;
recording those would make the Explain panel a pure ledger read. Rejected: four hundred
thousand "this frame is sharp" rows per wedding is a ledger nobody can search and a size
budget nobody can meet. Analysis is evidence *underneath* a decision, and every one of those
phases wrote that sentence into its own exit report first.

**Calibrating at read time.** Store the raw number only and map it on the way out, so a new
calibration improves history retroactively. Rejected for the reason section 6.3 gives about
the whole table: the ledger records what happened, and a row whose confidence changes when a
release ships is a row that cannot support a complaint about last year.

## 11. Consequences

* Every phase from 14 onward records decisions through `ExplainService` and inherits the
  explanation gate, the bands and the replay for free.
* Nothing in this build can act unattended: with no fitted calibration, the third multiplier
  puts every decision at `AutoZeroTouch` or below. Phase 28 cannot ship until a calibration
  does.
* The reason-code reference is generated from the registry, so a phase that adds a code and
  forgets to regenerate fails `tests/eval/explain_eval.rs` rather than shipping an
  undocumented reason.
* `contracts.lock` gains `ledger.rs`, migration 13 and the extended IPC surface. Changing any
  of them needs an ADR and a re-lock, in that order.
