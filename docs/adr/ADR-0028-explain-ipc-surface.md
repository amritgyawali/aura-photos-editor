# ADR-0028 - The explainability IPC surface

**Status:** accepted · **Date:** 2026-08-17 · **Phase:** 13 · **Supersedes:** nothing

## 1. Context

Phase 13 adds eight commands to the frozen IPC surface: six reads, one that records the
stored gallery's decisions into the ledger, and one that exports a support bundle. The
surface is frozen and `contracts.lock` covers it, so this ADR is the record required before
the re-lock.

## 2. Decision: nothing on this surface changes a decision

There is no command here that keeps, rejects, applies, swaps or edits anything. A
photographer who disagrees with an explanation changes the *decision*, on the culling
surface, and the ledger then records a new decision that supersedes the old one.

This is the boundary that makes section 12's first failure mode - "explanations drift from
actual behaviour" - structurally impossible rather than merely tested. An explanation the
interface could edit is an explanation that can disagree with what happened.

The one write, `record_decisions`, does not decide anything either: it reads the stored
selection through `CullService` and records what is already true.

## 3. Decision: the backend reads the vocabulary, the interface draws it

Every `LedgerReasonDto` carries `severity` and `domain` as the backend read them from the
registry. Every `LedgerDecisionDto` carries `autonomyTitle`, `autonomyText` and `calibrated`.
Every `ExplainTabDto` carries `available` and, when it is not, `unavailableReason`.

The alternative was a web view that knew the vocabulary. It would need a table of
ninety-three codes and their severities, a copy of the four autonomy bands and their wording,
and a list of which phases exist. All three go stale, and the first one goes stale in the
direction that matters: a view that decided for itself whether `keypoints_unavailable` is bad
news could tell a photographer their photograph is badly framed because AURA did not look at
it.

## 4. Decision: an unavailable tab says why, and is never absent

Six tabs always. Four read a frozen service and two - Edit and Quality check - belong to
phases that do not exist, so they render a sentence explaining that rather than nothing.

A blank tab reads as "there is nothing to say about the edit". What is true is that nothing
has edited anything yet, and those are different facts. The same rule applies to a
photograph with no technical verdict: the tab says AURA has not checked it *and that this is
not a judgement about it*, which is `AURA-ML-5050`'s argument one surface further out.

## 5. Decision: the inputs hash crosses as hex text

JavaScript cannot hold a `u64` exactly, and a support case quoting a rounded hash is a
support case about the wrong run. The same decision the culling surface made about
`deterministic_hash`, for the same reason, in the same format.

## 6. Decision: `record_decisions` is append-only and therefore not idempotent

Running it twice records two rounds of decisions. That is correct: a second cull genuinely is
a second decision, and the ledger's whole design is that history accumulates. Compaction is
what bounds it.

The DTO carries `refused` for the count of decisions that could not explain themselves. It is
on the wire rather than only in a log because a gallery whose reasons went missing is a
gallery whose panel will be empty, and the photographer should be told once rather than
discovering it one frame at a time.

## 7. Decision: the bundle crosses as a string, and the interface writes the file

`export_support_bundle` returns the anonymised JSON rather than a path. The interface decides
where it goes, and the photographer sees it before it goes anywhere.

A command that wrote a file would be a command that put a wedding's decision history
somewhere on disk without anybody looking at it. The scan result (`safe`) crosses beside the
text so the interface can refuse to offer a file the backend already distrusts - which has
never happened and is checked anyway.

## 8. Consequences

* `ui/src/ipc/types.ts` and `crates/aura-app/src/contract/ipc.rs` are re-locked together, as
  every phase since 01 has re-locked them.
* Phases 14, 20, 27, 29 and 30 add decision *kinds*, not commands: the panel, the queue, the
  replay and the bundle already read all six.
* The Explain panel has no route to a photograph's pixels. It asks the preview service for a
  crop of an already-cached proxy, which is why nothing in this phase can leak one.
