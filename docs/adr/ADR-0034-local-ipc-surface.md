# ADR-0034 - The local light IPC surface

**Status:** accepted · **Date:** 2026-08-19 · **Phase:** 19 · **Supersedes:** nothing

The second of phase 19's two ADRs. [ADR-0033](ADR-0033-local-light-sculpting.md) covers the
decisions; this covers the wire, which is a frozen contract in its own right -
`ui/src/ipc/types.ts` is in `contracts.lock` and changing it needs a re-lock.

## 1. Context

Six commands. Three read - the project's coverage, one photograph's plan and the
low-confidence review queue - one runs the resumable pass, and two record what the
photographer decided about a frame the pass had already decided.

The surface has one problem no previous phase's had. **This phase's whole success condition is
that its work is invisible**, and a panel is where that becomes a liability rather than an
achievement: a photographer who cannot see what was done cannot decide whether they agree with
it. So this surface carries more explanation per decision than any before it, and section 3
below is about the two fields that exist only for that.

## 2. Decision: no command returns a mask, and no DTO could hold one

`LocalPlanDto` has no alpha, no matte, no grid and no field that could carry one. Neither does
any other type on this surface.

That is the same boundary [ADR-0033](ADR-0033-local-light-sculpting.md) section 4 draws in the
decision layer, drawn again here because the wire is where it would most plausibly be crossed:
a panel that wanted to *show* a mask overlay would ask for one, and the shortest path to that
is a base64 field on the plan. Phase 18 owns masks; when it ships it will have its own surface
and its own overlay, and phase 19's panel will link to it rather than duplicate it.

What the panel gets instead is enough to draw the work without the mattes:

* the **reasons' own evidence rectangles**, which is where a shine reduction happened and
  where a face was lit;
* the **shaping zones by name**, which is what a retoucher would call the moves;
* the **gates**, as operation-and-mask-kind pairs, which is what turns "nothing happened here"
  into "AURA could not find the background".

## 3. Decision: two fields that exist because the edit is invisible

**`FaceLightDto::noiseCapEv`.** What the lift *could* have been, beside what it was. This is
the single most useful field on the surface and it is not a decision - it is the bound that
stopped one. "AURA lifted her face 0.4 EV and would have lifted it 0.9, because lifting
further would have brought out grain" is a sentence a photographer can act on; "+0.4" is a
number they argue with.

**`LocalStatusDto::actedOn`.** The fraction of planned frames where at least one operation
actually ran, beside `coverage`, which is the fraction that were planned at all. Every phase
since 05 reports coverage; this is the first that needs a second number, and the reason is the
invisibility again: a wedding at 100 % coverage and 4 % acted-on looks *exactly* like a wedding
that was worked on. Without this field there is no way for a photographer to find out
otherwise short of opening frames one at a time.

`maskCovered` is the third of the same family and the one that says whether phase 18 is doing
its job. On this build it is zero.

## 4. Decision: `operations` and `opNames` are sent rather than hard-coded

Both `LocalPlanDto` and `LocalStatusDto` send the operations' stable slugs beside the arrays
indexed by them, and `LocalStatusDto` does the same for the mask kinds.

It costs about sixty bytes a response and it removes a class of bug the previous phases have
all had: a panel that hard-codes the order of a histogram is a panel that renders the wrong
labels the day an enum gains a variant, silently, and looks completely plausible while doing
it. Sending the names makes adding an operation one change rather than two that can disagree.

## 5. Decision: a strength override writes the whole mask list, and the merge refuses more here

`set_local_strength` records the photographer's own strength through `LocalService` and then
writes the plan into the recipe through `aura_recipe::schema::merge`, exactly as phase 15's
tone override does. One thing is different and it is worth knowing about.

`schema`'s own rule is that **arrays are atomic**: `masks` is one path, not a path per
element, because a per-element path would break the moment somebody reordered two masks. So a
photographer who has edited *any* mask by hand owns the whole list, and an automated local
pass is refused entirely for that frame rather than merged into.

That is stricter than the tone case, where a person who set the temperature keeps the
temperature and gets the product's exposure. It is the right side to err on: half of somebody's
own local edits replaced by half of AURA's is not a state anybody asked for, and it is not a
state a photographer could reason about when they saw it.

`recipesProtected` on `LocalPassDto` counts the refusals so a pass that quietly did nothing to
four hundred frames is visible rather than silent.

## 6. Decision: `sculptLocal` takes a list of photographs, and that is the normal path

`SculptLocalInput::photoIds` empty means "every photograph with no current plan". It is
supported and it is not what the job graph does.

Invariant 3 is three-tier compute: expensive work only on survivors. In every phase before
this one that has been an optimisation. Here it is the design, and section 11 says so in its
own budget - the third row is "**1,000 selected images** total <= 90 s", written about a
gallery rather than about a wedding. Local light sculpting on four thousand frames of which
three thousand will never be delivered is eighty milliseconds each spent on nothing.

## 7. Decision: no command on this surface culls, retouches or normalises

Three boundaries, all structural rather than remembered:

* **no keep, reject, deliver or cull.** Phase 12 owns delivery. `LocalPanel.test.tsx` asserts
  the rendered panel contains none of those words, which is the same test phase 10's moment
  browser carries.
* **no blur, smooth, radius or texture parameter.** Phase 20 owns skin texture, and the shine
  reduction is a luminance operation whose type cannot express anything else.
* **nothing reads a second photograph.** Phase 25 owns gallery consistency.

## 8. Consequences

**Good.** The panel can explain an invisible edit, which is the thing this phase most needed
and the thing a screenshot of six sliders would not have given. The gates are visible as
"AURA could not find the background" rather than as an operation sitting at zero.

**Bad.** `LocalPlanDto` is the largest DTO in the product - thirty fields plus four nested
arrays - and most of it is explanation rather than decision. That is the cost of the
invisibility and it is paid on every frame the panel opens.

**The thing to watch.** `strengths` and `operations` are parallel arrays and nothing on the
wire enforces that they are the same length. A panel that trusted the index without checking
would render a slider under the wrong label. `LocalPanel.tsx` reads them together and the test
covers it; a second consumer will have to do the same.
