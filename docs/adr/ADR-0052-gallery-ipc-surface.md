# ADR-0052 - The gallery consistency IPC surface: nine commands, two denominators, and the four things a photographer can say

**Status:** accepted · **Date:** 2026-08-30 · **Phase:** 25 · **Supersedes:** nothing

[ADR-0051](ADR-0051-gallery-consistency-and-normalisation.md) records what the consistency engine
decides. This records what the window can ask it, and - more usefully - what it deliberately
cannot.

## 1. Context

Phase 25 section 4 asks for four panels: `ConsistencyView`, `TimelineStrips`, `AnchorPicker` and
`OutlierList`. Section 9 gives SFE "consistency view, timeline strips (before/after), anchor
picker, outlier list" and MFE "node tree navigation, per-node overrides, batch accept".

This is the first panel in the product whose subject is **a wedding rather than a photograph**.
Every develop panel since phase 14 answers "what is happening to this frame"; this one answers
"what is happening to these four hundred frames, and which of them does not belong". That changes
what the wire has to carry, and it changes what a badly designed surface would let a photographer
believe.

## 2. Decision: nine commands, and the shape of each

| Command | Reads or writes | What it is for |
|---|---|---|
| `gallery_status` | read | The project header: coverage, nodes, anchored nodes, spread before and after, the worst skin spread |
| `gallery_nodes` | read | The node tree, in capture order, with each node's target and reasons |
| `gallery_node_strip` | read | One node's deltas in capture order - what a timeline strip draws |
| `image_gallery` | read | One photograph's delta, its reasons and its residual |
| `gallery_outliers` | read | The QC queue, worst first, with the sentence section 6.4 asks for |
| `gallery_pass` | write | Run the consistency pass over a project |
| `pin_gallery_anchor` | write | Pin or reject one anchor, and re-solve that node |
| `set_gallery_override` | write | Record what the photographer set instead, on one frame |
| `disable_gallery` | write | Switch the pass off for one frame, or back on |

Four reads, one pass and four decisions. `gallery_node_strip` is a separate command from
`gallery_nodes` for the reason phase 18 separated its mask payload from its outline: a wedding has
forty nodes and four thousand frames, and a panel that listed the tree would otherwise pull every
delta in the project to draw a header.

## 3. Decision: both denominators are on the wire, and neither is computed in the panel

`GalleryStatusDto` carries `photos`, `normalised`, `nodes` **and** `anchoredNodes`. A project at
100 % coverage and 20 % anchored has had almost nothing done to it - an unanchored node produces a
zero delta for every frame in it, and a zero delta is still a row - so a panel that showed only
coverage would render a wedding nobody could judge as a wedding that needed no work.

Phase 05's rule, inherited for the nineteenth time and at its most consequential here, because this
is the phase where a green number and an untouched gallery look identical.

The same applies to the headline claim. `spreadBeforeCct` and `spreadAfterCct` are both on the wire
and the *reduction* is computed from them, rather than a percentage being sent. A panel that
received "77 % reduced" could not tell 500 K → 115 K from 20 K → 4.6 K, and only one of those is
worth telling a photographer about.

## 4. Decision: four things a photographer can say, and no fifth

Pin an anchor. Reject an anchor. Set a frame's own movement. Switch the pass off for a frame.

**There is no strength slider, no damping field and no way to raise a bound anywhere on this
surface.** `GalleryOverrideInput` carries five optional movements, every one of them bounded by the
frozen contract, and `Gallery::set_override` refuses a value outside its bound rather than clamping
it. Phase 21's rule - a ceiling can be lowered by a studio and raised by nobody - applied to a
surface a person touches: a photographer who wants a frame moved further than 450 K is a
photographer whose *per-frame* estimate is wrong, and phase 15's own override is where that is
fixed.

There is also **no batch accept of deltas**, despite section 9 giving MFE "batch accept". What
ships is batch accept of a *node's* anchors - `pin_gallery_anchor` per frame from a multi-select -
because a delta is not a thing to accept: it is already stored, and what a photographer accepts is
the anchor choice that produced it. A command that marked four hundred deltas accepted would be
four hundred rows saying a person looked at something they scrolled past.

## 5. Decision: the pass is a command, and it says what it did rather than what it will do

`gallery_pass` runs to completion and returns a `GalleryPassDto`: nodes, anchored, split,
normalised, outliers, the two spreads and the elapsed time. It does not return a job id.

That is different from phases 06 to 24, whose passes are per-photograph and resumable per row, and
the difference is ADR-0051 section 2's: **resumability here is at the level of the pass**, because a
node half-solved against one target and half against another has a target that describes neither.
There is no meaningful progress state to poll that is not a lie about what a reader could do with
the catalog at that moment.

The command is still cancellable, and a cancelled pass writes nothing.

## 6. Decision: the panel is told what this build cannot do

`GalleryStatusDto.skinFieldAvailable` is `false` in this build and is **on the wire rather than
inferred**, exactly as phase 24 put `detectorTrained` there. Phase 18's segmentation head is
untrained, so no photograph has an identity-scoped skin region, and every frame records
`SkinMaskAbsent`.

A panel that had to infer it from `skinTargeted == 0` would eventually render "everybody's skin is
consistent across this wedding" for a build that cannot look at skin. That sentence is the single
most damaging thing this product could say wrongly, because it is a promise about people.

## 7. Decision: an outlier arrives with its sentence already assembled

`GalleryOutlierDto.description` carries "+310 K warmer than the anchors, skin cast 4.2 dE00",
rendered by `Outlier::describe` from the residuals. The residuals are on the wire too, so a panel
can draw them; the sentence is there so the panel does not assemble a second version of it that
drifts from the one phase 27's QC ticket will show.

Reason *codes* are on the wire as slugs plus their rendered text, which is the shape every panel
since phase 09 has taken: the code is what a filter matches on and the text is what a person reads,
and neither is stored.

## 8. What is deliberately not on this surface

**No pixels.** Nothing here returns an image, a thumbnail or a strip bitmap. `TimelineStrips` draws
its before-and-after from the numbers - a strip of temperature swatches is a strip of `<div>`s with
a background colour computed from a kelvin value - because a wedding's worth of strip images is
tens of megabytes over a channel that exists to carry decisions. Phase 13's rule that evidence can
never be a pixel, read one step further.

**No apply.** There is no command that writes a recipe. `aura-app` merges an accepted delta through
`aura_recipe::schema::merge` when the develop panel renders a frame, which is the only function in
the workspace permitted to write a recipe and the only place `user_edited_fields` is honoured.
Adding an `apply_gallery` command would be a second way to edit a photograph.

**No node editing.** A photographer cannot split, merge or re-parent a node. A node is a *lighting
group* the product measured, not a chapter somebody named - phase 07's `StoryService` owns chapters
and has `split_segment`, `merge_segments` and `set_chapter` for exactly this. A second editable
tree would be a second answer to what a wedding's structure is, and the two would drift.

**No cloud anything.** Section 7: the gateway stays idle in this phase.
