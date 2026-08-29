# ADR-0050 - The cleanup IPC surface: nine commands, none of which can carry a strength or a description

- **Status:** accepted
- **Date:** 2026-08-29
- **Phase:** 24 - Generative Cleanup & Distraction Removal
- **Supersedes:** nothing
- **Related:** ADR-0049 (the safety engine), ADR-0046 (the micro-retouch surface, whose "a ceiling
  can be lowered by a studio and raised by nobody" this inherits), ADR-0048 (the restore surface,
  whose "it publishes what was declined" this takes further)

## 1. Context

Phase 24 needs a command surface. Every earlier one answers *what did AURA do to my photographs*.
This one has to answer **what would AURA take away**, and on the build that ships the honest answer
is "nothing, and here is exactly why" - which turns out to change the shape of the surface rather
than only its contents.

Section 9's SFE row asks for a proposal queue with before/after, accept and reject, and a manual
removal tool. Section 13's fifth acceptance criterion asks that every cleanup be disclosed in the
recipe and the delivery report. Section 2.2 puts removing a guest out of scope as an automated
feature and permits it "only as a manual tool with explicit confirmation".

## 2. Decision: nine commands, and the refusals are one of them

| Command | What it does |
|---|---|
| `cleanup_status` | The project header: coverage, the blocked histogram, and `maskCovered` |
| `image_cleanup` | One photograph's proposals, strongest first |
| `cleanup_blocked` | One photograph's **refusals**, with the check that made each |
| `cleanup_disclosures` | Everything removed from the project, for the delivery report |
| `cleanup_pass` | The resumable pass |
| `decide_cleanup` | Accept or reject one proposal |
| `disable_cleanup` | Leave one photograph alone entirely |
| `manual_remove` | Remove one region a person drew, after they confirm |
| `cleanup_reason_codes` | The panel's legend, from the frozen enum |

`cleanup_blocked` is the one that would not exist on an ordinary surface. It is here because more
than half of `CleanupCode` is refusals, section 10.1's adversarial audit is scored from those rows,
and teaching a photographer what AURA will never do is most of the trust this feature needs. Phase
22 published its identity refusals for a similar reason and this goes further: there, the refusals
were a minority of an otherwise ordinary decision; here they are the majority of what the product
has to say.

It is a **separate call rather than a field** on the proposal list, because the refused set is
usually larger than the proposed one and a queue that fetched forty refusals to draw three
proposals would be slower for no benefit.

## 3. Decision: `maskCovered` is the headline, and the panel leads with it

`CleanupStatusDto.maskCovered` is the fraction of *examined* frames on which all six protected
kinds could be looked for. On this build it is zero, for a reason ADR-0049 section 3 records and
this surface has to make visible: phase 18's twenty mask classes contain **no word for a ring or a
cake**, so a coverage assembled from them is never complete, so every candidate is refused with
`protection_unknown` rather than allowed.

A panel that led with "0 suggestions" would let a build with no segmenter look exactly like a build
that examined every photograph and found them all clear. Those are different rows, different reason
codes and different runbooks - `AURA-ML-5122` against `AURA-ML-5116` - and only one of them is a
claim about the photographs.

So `ProposalQueue` chooses its headline sentence from the coverage rather than from the proposal
count, and the figure is labelled "could check for people" rather than given a percentage without a
noun.

The denominator is **examined** frames rather than every photograph, which is a departure from
phase 09's rule and is deliberate: a frame nobody has looked at has no mask answer either way, and
counting it as an incomplete mask would report a project that has not run yet as one whose
segmenter is failing.

## 4. Decision: accepting is not applying, and applying is not on this surface

`decide_cleanup` marks a proposal accepted. Nothing about it writes a recipe or replaces a pixel.

What turns an accepted proposal into replaced pixels is `CleanupStore::apply`, which writes the
disclosure and sets the applied flag in one transaction, with a trigger that aborts the second half
if the first is missing. **No command on this surface calls it**, and that is a statement about this
build rather than about the design: nothing here produces a proposal to apply, because there is no
trained detector and no mask coverage. Conditions C1 and C2 of the exit report.

When those close, the command that applies belongs in `aura-app` beside the develop surface, because
it needs `aura_recipe::schema::merge` and the stored patch - neither of which `aura-generative`
can reach.

One command that did both would be simpler and is what most products ship. The failure it invites is
specific: a panel marks a proposal accepted, the render fails, and the catalog now carries a
disclosure saying a removal happened to a photograph that still has the bin in it. A disclosure that
is not true is worse than no feature.

## 5. Decision: nothing on this surface carries a strength, a size or a description

There is no `strength` field, no `radius`, no `feather`, no `area_cap` and no text field of any kind
on any input in this surface. The three things a person may say are **yes**, **no**, and **leave
this photograph alone**.

That is not minimalism. `docs/generative-policy.md` promises that AURA never generates from a
description, and the way a promise like that is kept is that **no type on the path could carry
one** - which is checked by `crates/aura-generative/tests/one_choke_point.rs` in the engine and is
true here by inspection of the DTO block. A surface with a prompt field is a surface where the
promise has become a default somebody can change.

The caps are the same shape, and it is phase 21's rule restated: the contract owns
`AREA_CAP_DEFAULT`, `DENYLIST_OVERLAP_MAX` and `ZERO_TOUCH_CONFIDENCE`, `cleanup_policy.toml` may
only tighten them, and nothing on the wire touches any of the three.

## 6. Decision: the manual tool runs the whole safety engine, and cannot remove a person

`manual_remove` takes a rectangle a person drew and puts it through all five checks in
`SafetyCheck::ALL` order. A person choosing a region is a reason to skip the *detector*, not a
reason to skip the filter.

Two consequences worth stating explicitly:

- A hand-drawn region is `DistractionClass::Unclassified`, which `story_safe` refuses, so on this
  build a manual removal comes back as a refusal that names the missing detector. That is the
  correct behaviour for a build that cannot tell what is inside the box.
- **No confirmation makes a person removable.** The safety engine refuses `BackgroundPerson`,
  migration 24 has a CHECK that refuses the class outright, and a trigger refuses an UPDATE into it.
  The confirmation this command collects is confirmation that a *photographer* wants a region gone;
  it is not permission for AURA to decide that somebody is not part of a wedding.

`confirmed` is a **field on the wire** rather than an implication of calling the command, because
section 2.2 asks for explicit confirmation and a call is not one. A future UI that dropped the
dialog gets a refusal rather than a removal.

## 7. Decision: a refusal is a result, not an error

`manual_remove` returns `ManualRemoveDto { proposal, blocked }` rather than an `Err` when the safety
engine declines. A refusal is the product working; rendering it through the error path would put it
in the problems panel beside disk failures and would make the commonest, most correct outcome of
this command look like a fault.

The error path is reserved for the two things that genuinely are faults: an unreadable proxy, and a
call with `confirmed: false`.

## 8. Decision: no command returns pixels

Phase 13's rule, eleventh surface running. `BeforeAfter` takes two image sources and never fetches
one: the develop view already renders a photograph at a region, and this asks it for the same region
twice - once with the recipe's `cleanup[]` and once without. What the cleanup surface adds is the
*rectangle and the method* those two renders differ by.

The component also draws the region outline over both, which is a small decision with a real
reason: **a removal that works is invisible**, so a before-and-after of a good one looks like two
identical photographs, and without the outline a photographer cannot tell a subtle repair from a
control that failed to load.

## 9. Consequences

- The product's IPC surface goes from 180 commands to 189, and the three-way count -
  `#[tauri::command]` definitions, `generate_handler!` registrations, typed client wrappers - stays
  equal at 189. The script that asserts the equality is what stops phase 21's ninety-unhandled-call
  defect from happening again.
- `aura-app` gains `aura-generative` as a dependency. It is the crate where the editorial-judgement
  port meets `aura-cloud` and where an accepted proposal meets `aura_recipe::schema::merge` - the
  two edges `aura-generative` deliberately does not have.
- Three panels ship and, like every develop panel since phase 12, **no view mounts them yet**. That
  is the standing UI-shell gap rather than this phase's, and it is recorded in the exit report.

## 10. What was considered and rejected

**One `set_cleanup_override` command carrying every decision.** Rejected in section 4. The shape
invites a caller to accept and apply in one step, which is exactly the failure that produces an
untrue disclosure.

**Returning the blocked candidates on the proposal list.** Rejected in section 2. It would make the
common query as expensive as the rare one.

**A `strength` on the manual tool, so a photographer could ask for a lighter touch.** Rejected in
section 5. A removal is not a strength - the object is either gone or it is not - and the field
would exist only to be the first thing a "creative fill" feature reached for.

**Letting `manual_remove` skip the denylist when the photographer confirms.** Rejected in section 6,
and it is the one rejection in this document that somebody will propose again. The argument for it
is that a professional drawing a box around a stranger's elbow knows what they are doing. The
argument against is that the denylist is not protecting the photographer from a mistake; it is the
mechanism by which `docs/generative-policy.md` is true, and a confirmation dialog that switches it
off makes the document a description of a default.
