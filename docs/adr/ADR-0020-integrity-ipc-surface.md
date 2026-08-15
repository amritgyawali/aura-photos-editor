# ADR-0020 - The integrity IPC surface

**Status:** accepted
**Date:** 2026-08-15
**Phase:** 09
**Deciders:** CTO, Senior Frontend Engineer, Product Manager

---

## 1. Context

Phase 09 produces a technical verdict for every photograph. Six later phases read it and
one interface shows it, and this ADR freezes the shape both sides agree on:
`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts`, checked by
`cargo xtask contracts`.

The surface is small - six commands - and the reason is the whole decision.

---

## 2. Decision 1: five reads and one write

| Command | What it does |
|---|---|
| `integrity_status` | the panel header: coverage, the flag histogram, the uncalibrated bodies |
| `image_integrity` | one photograph's verdict, or `null` when nobody has looked |
| `flagged_images` | the frames carrying any of these marks, worst score first |
| `within_moment` | one moment's frames ranked by subject sharpness |
| `dismiss_flag` | **the only command that changes anything** |
| `analyse_integrity` | the pass; a job rather than a click handler |

Phase 08's surface had nine commands and five of them changed a grouping. This one has
six and exactly one changes a stored fact.

**There is no command here that keeps, rejects, ranks or orders a photograph for
delivery.** Section 2.2 puts every one of those in phase 12, and this is the surface where
crossing that line would be easiest: a `technicalScore` sorted descending *looks* exactly
like a cull, and the interface that shipped it would be right about the numbers and wrong
about the product.

`within_moment` comes closest and deliberately stops short. It answers the question phase
12 asks most often - "which of these six is sharpest" - and it says nothing about which of
them a client sees.

---

## 3. Decision 2: `null` means nobody looked

`image_integrity` returns `Option<IntegrityDto>`, and the card draws a different thing for
`null` than for a clean verdict.

Migration 9's fifth property is that "not checked" is not "clean", and the reason it is a
schema property rather than a convention is that phase 12 reads a clean verdict as
*evidence*. Carrying that distinction out to the interface is what stops a photographer
reading an empty card as "AURA checked this and found nothing".

---

## 4. Decision 3: the backend says which marks are the good news

`IntegrityDto::hasDefect` and `IntegrityReasonDto::exoneration` are computed in Rust and
sent, rather than derived in the interface from a list of slugs.

Two of the fourteen flags - `intentional_motion` and `eyes_closed_ok` - describe something
*right* with a photograph, and a third, `no_subject_detected`, withdraws a claim rather
than making one. An interface that worked that out for itself would work it out wrong
exactly once, and the failure would be a panned exit frame drawn in the same red as a
blown dress.

For the same reason `IntegrityStatusDto` carries `flagNames` beside `flagCounts`: a chip
list written as a literal in the UI is a second copy of `IntegrityFlags::ALL`, and the
first time somebody adds a fifteenth flag the two disagree *silently* - a missing chip
looks like a wedding with no such frames in it.

---

## 5. Decision 4: both denominators, always

`IntegrityStatusDto` carries `photos`, `scored`, `coverage` **and** `subjectAware`.

Phase 05's rule inherited a fifth time, with this phase's refinement. The denominator of
`coverage` is every photograph - unlike the moments view's, where a frame with no
embedding is a phase 05 gap. A technical verdict needs only pixels, so a frame with no
verdict is *this* phase's gap.

`subjectAware` is the second number and it is the one that matters most. A wedding at
100 % coverage and 2 % subject-aware has been judged on frame-wide sharpness nearly
everywhere - which is the ordinary global measure this phase exists to replace - and a
caller about to trust `subjectSharpness` has to be able to find that out.

`defectiveAtMost` is named for its own imprecision: one frame can be soft *and* noisy, so
the chip counts overlap and their sum is an upper bound. A photographer who adds four
chips together and gets more than their whole wedding has been told something untrue.

---

## 6. Decision 5: `gatingFaces` travels with `closedEyeRatio`

Zero over zero and zero over six are different facts. The first is "nobody's eyes were
judged" and the second is "nobody blinked", and a panel that blurred them would say
"eyes open" about a frame in which nobody was looked at.

The same rule the store applies and the same rule the card renders: every eye sentence in
the interface states its denominator.

---

## 7. Decision 6: `dismiss_flag` promises exactly what it does

It clears one flag, records that the photographer disagreed, and is not undone by a
re-analysis - the dismissal is re-applied to the freshly computed flags inside the
statement that stores them.

It does **not** change whether the photograph is delivered, and the card says so in
those words next to the button. The three flags that are not faults cannot be dismissed:
there is nothing to forgive and offering the button would suggest otherwise.

The command takes exactly one flag. Clearing "soft and noisy" in one statement would
record one decision for two independent disagreements, and the review history could not
say which one the photographer meant.

---

## 8. Decision 7: the events are typed and not yet emitted

`IntegrityEvent` mirrors section 11's three telemetry events and is typed on both sides.
Nothing emits it, for the reason `MomentEvent`, `StoryEvent` and `PeopleEvent` are not
emitted either: the Tauri shell has not been launched on the development machine, so an
emitter would be code nobody has run. The three are `tracing` spans today and this is
their wire shape for when the shell runs.

---

## 9. Consequences

**Good.** Six commands, one of which writes. The boundary section 2.2 draws is visible in
the command list rather than only in a document. The interface cannot invent a defect
class, cannot mislabel an exoneration, and cannot report a coverage without its
denominator.

**Bad.** `flagged_images` returns ids rather than rows, so the grid does a second read to
draw them. That is the same shape `find_similar` has and the same trade: a command that
returned rows would either duplicate `list_images` or invent a third row shape.

**Ugly.** `analyse_integrity` blocks for as long as a wedding takes - about eight and a
half minutes for four thousand frames on the development machine. It is a job, it is
cancellable through the token id, and the 50 ms rule in `aura-app`'s own header is
explicitly the rule this one command is the exception to. The header says which command
that is.
