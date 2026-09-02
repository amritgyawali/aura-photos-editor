# ADR-0061 - Delivery, the learning loop, and release engineering

**Status:** accepted
**Date:** PHASE-30
**Deciders:** CTO, TLC, MLL, MLOPS, DEVOPS, SEC, PM
**Supersedes:** nothing. **Amends:** ADR-0029 section 9 (the RENDER error block).

## Context

Phase 30 is the last phase, and it is three features that share one property: each of them is the
first thing in this product to leave the catalog.

Export writes files. Delivery sends them to a drive, a NAS or somebody else's server. The learning
loop changes what the product will do to the *next* wedding. Twenty-nine phases produced rows, and
a row that is wrong is a row somebody re-runs. None of these three has that property.

That asymmetry is the whole of this ADR.

## Decision 1 - A hash that was not read back is not a hash

`ExportedFile::hash` is defined in the contract as the digest of the bytes **re-read from the
destination**, never of the buffer that was written. `DeliveryManifest` can only be assembled out of
`ExportedFile`s.

The alternative - hash the buffer on the way out, which is free because the bytes are already in
memory - is what most exporters do, and it catches nothing. A short write, a full volume whose
filesystem reported success, a NAS that acknowledges and drops, and a failing SD card all produce a
correct in-memory buffer and a wrong file on disk. Section 6.1's first sentence is "photographers
have lost galleries to silent write failures"; hashing the buffer would let us make the same claim
while detecting none of them.

The cost is a full re-read of every file, which section 11 budgets at 8 % of export time. It
measures at about 6 % on this machine.

## Decision 2 - `verify` stays a field, and turning it off is visible in three places

Section 5 froze `ExportJob { …, verify: bool }`. Section 6.1 says verification is mandatory by
default. Those are in tension, and the resolution is not to delete the field.

A constant `true` would have been cleaner and would have been dishonest about one real case: a
photographer re-exporting 4,000 frames to a scratch volume to check a naming template does not need
a read-back, and making them wait for one teaches them to distrust the setting that matters. So the
field stays, `ExportJob::new` sets it to `true`, and a job that ran without it is counted on
`ExportOutline::unverified`, recorded per file on `ExportedFile::verified`, and stated in the
manifest's own header.

The rule the product follows is not "verification cannot be switched off". It is "**a delivery that
was not verified can never look like one that was**".

## Decision 3 - A failed verification halts the job

`AURA-RENDER-8022` is `run_blocking` / `halt`, which makes it one of the few item-level failures in
this product that stops a run.

Everywhere else - phases 09 to 29 - the right answer is to skip the item and carry on, because the
alternative is a wedding that fails on its 3,000th frame. Here it is the opposite. A gallery missing
one photograph is a phone call. A gallery containing one corrupt photograph is a photograph nobody
notices until the couple opens it, six weeks later, after the originals have been archived.

And a verification failure is almost never about the file. It is about the volume, which means the
next 300 files are at the same risk.

## Decision 4 - A destination is a place; a provider is a trait; and nothing here opens a socket

`Destination` names *where*. Nothing in the frozen contract says *how*. That is what makes section
6.2's "adding a provider must not touch core code" true rather than aspirational.

Underneath it, `aura-delivery` splits a provider into two things: a `Provider` (what a service's
collections are called, how a set maps onto them, what its digest header is) and a `Transport` (put
bytes, ask what arrived). Two providers ship, over a transport port with two implementations: a
filesystem transport, which is real and is what a folder, a NAS and an external drive use, and a
recorded transport for tests.

**No HTTP transport ships in this build**, and that is a lint rather than an omission.
`scripts/check-banned.sh` refuses `std::net`, `reqwest`, `hyper`, `rustls` and six more outside
`aura-cloud`, and phase 04's rule is that one crate owns outbound networking. A gallery provider is
not a model provider, so the honest options were to widen that lint or to ship the state machine and
the port and leave the socket for the phase that widens it. We took the second: the resumable upload
logic, the per-file state, the digest comparison and the mapping are all real and all tested against
a transport that can be made to drop connections on demand. What is missing is one implementation of
a two-method trait. It is condition C3 in the exit report.

## Decision 5 - `Correction` is section 5's shape, and everything the store needs is beside it

Section 5's `Correction` has eight fields and none of them is the project, the image, or which value
moved. It cannot bucket, it cannot hold out, and it cannot count weddings.

We did not add fields to a frozen shape. `CorrectionContext` sits beside it and carries the four
things the store needs. The alternative - amending the frozen struct - would have been the sixth
contract amendment in the product's history, and unlike the other five it would have been for the
convenience of the implementation rather than because the shape was wrong.

## Decision 6 - The held-out split is deterministic, by a hash of the correction's own id

Not a shuffle, not a timestamp cut, not "the last quarter".

A shuffle re-draws the split on every fit, which means a fit whose measured improvement is
disappointing can be re-run until the line falls somewhere flattering - and nothing about that would
look wrong in a review, a test, or a panel. It is the single easiest way for this feature to become
a number generator.

A timestamp cut is worse in a subtler way: it holds out the most recent corrections, which are
exactly the ones a photographer's *current* taste is in, so the candidate is measured against the
taste it is trying to learn.

`HeldOut::deterministic` is on the wire so the panel can say the split was reproducible.

## Decision 7 - `Learnable` is closed, and what is absent is the guarantee

Fifteen members, no `Other`. Style deltas, two ranker weights, two threshold offsets, one curation
threshold.

There is no member for a mask boundary, a retouch texture floor, a crop safety margin, a cleanup
permission, a skin guard, an identity-drift cap or a coverage rule. Those are *guarantees* rather
than preferences. A photographer who repeatedly widened a retouch is a photographer whose next
wedding must still get the texture floor, because the floor is a promise `docs/retouch-ethics.md`
makes about the product rather than a default somebody chose.

A loop that could move one would learn its way past a promise, one wedding at a time, with every
gate green - and the phase that noticed would be a phase with no way to tell which weddings had been
delivered under a floor that had drifted. `crates/aura-core/src/contract/learn.rs` has a unit test
asserting no member's name contains any of eight words a guarantee is spelled with, and
`crates/aura-learn/tests/no_guarantee_learning.rs` is the tenth grep-as-a-test in this repository.

## Decision 8 - Four error domains, and three of them are new

Six export codes are `RENDER`, because phase 14 renamed the reserved-but-empty `EXPORT` block to
`RENDER` with a note saying "phase 30 will want codes in the same domain for the same subject". An
export is a render written to a file.

`DLV`, `LRN` and `REL` are new. Three new domains in one phase is more than any phase since 01, and
the reason is that this phase adds three separate *areas*: getting a file somewhere else, changing
the product's own future behaviour, and shipping the application. Those fail differently, are
diagnosed by different people and have different runbooks. A support case that opens "my gallery did
not arrive" and one that opens "the update made it worse" should not be in one list.

## Decision 9 - Nothing is adopted without a person, and there is no code path that could be

`LearningUpdate::adopted` is set by `LearnService::adopt` and by nothing else. There is no confidence
above which an update adopts itself, no setting that enables one, and no autopilot stage that calls
`adopt`.

This is the same shape ADR-0050 gave phase 24's cloud judgement and ADR-0060 gave phase 29's
sequencing: the feature's failure modes are all in one direction. An unreachable machine, a bad fit,
a corrupt correction table, a photographer who never opens the panel - every one of them leaves the
profile exactly as it was.

## Decision 10 - Consent is per project, off, and recorded with the wording it was given to

`Consent` carries four separate switches and the app version that asked. Four rather than one because
"may this machine learn from this wedding" and "may anonymised evidence leave it" are different
questions, and collapsing them is how the second one happens by accident.

The app version is on the record because a consent given to one release's wording is a consent to
that wording. A privacy page that changes and a consent that does not is a consent that has quietly
become about something else.

## Consequences

* Export is slower than it would be by about 6 %, and the manifest is worth having.
* This build cannot upload to a real gallery provider. Everything except the socket is built and
  tested; C3 in the exit report says so.
* The learning loop cannot move a guarantee, ever, without an ADR that amends `Learnable` and a
  grep-as-a-test that has to be deleted first.
* Three new error domains means `docs/reason-codes.md` and the registry test both grew.
