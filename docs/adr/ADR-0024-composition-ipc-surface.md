# ADR-0024 - The composition IPC and overlay surface

**Status:** accepted  
**Date:** 2026-08-15  
**Phase:** 11 - Composition & Aesthetic AI  
**Supersedes:** nothing. **Amends:** nothing.

---

## 1. Context

Composition is unusually easy to present as taste masquerading as fact. A score without
its evidence looks like a verdict, a suggested crop looks like an edit, and a flagged
review queue looks like a cull. The application boundary must keep all three distinctions
visible while still making the feature useful.

The surface therefore exposes five commands: three reads, one narrow override, and one
resumable analysis request. It sends backend-derived meanings rather than asking the web
view to recreate thresholds or flag semantics.

## 2. Commands

| Command | Kind | Purpose |
|---|---|---|
| `composition_status` | read | Coverage, keypoint-aware coverage, flag names/counts, score/tilt summaries, unruled scenes, and versions. |
| `image_composition` | read | One complete reading and its evidence for the Explain card; `null` means not analysed. |
| `flagged_composition` | read | A bounded, project-scoped review queue matching any requested flag, worst score first. |
| `dismiss_composition_flag` | write | Record disagreement with one currently present defect and return the updated reading. |
| `analyse_composition` | work | Run or resume pending project rows, with cancellation and a typed pass report. |

There is deliberately no apply-crop, straighten, remove-object, keep, reject, deliver, or
gallery-order command. Phase 23 owns geometry actions; phase 12 owns selection.

## 3. Decision: `null` is “not checked”

`image_composition` returns `null` when no row exists. A clean analysed frame returns a
non-null reading with a `clean` reason and confidence. The card renders those states
differently because a missing preview, failed model, or interrupted pass must not turn into
positive evidence for phase 12.

The project header likewise shows both total photo count and scored count. Keypoint-aware
coverage is separate: a project can have every horizon measured while no limb crop was
audited.

## 4. Decision: semantics cross the boundary with the data

The Rust DTO includes `exoneration` on each reason, `flagged` on each joint cut,
`hasViolation` on the reading, and `actionable` on a crop hint. TypeScript does not copy the
penalty mask, cut threshold, or crop-confidence threshold. Flag names travel with the
parallel histogram so adding a backend flag cannot silently relabel a bar.

Stable reason and flag slugs remain on the wire for localisation and filtering. User-facing
text comes from the backend's closed vocabulary or a bounded angle/joint-specific template;
callers cannot invent psychological, selection, or object-identity claims.

## 5. Decision: the overlay uses normalised evidence

Evidence rectangles and crop-hint regions use 0..1 coordinates and render as percentages.
The same reading therefore aligns on a thumbnail, loupe, or resized panel. Frame-wide
reasons such as horizon and balance are labelled without a fabricated rectangle. The card
offers a thirds grid and measured horizon line as explanations, not controls.

The suggested region is dashed and labelled “hint”. It is never applied by this surface.
An unavailable hint is not replaced with a full-frame rectangle, because that would look
actionable while carrying no evidence.

## 6. Decision: review is not selection

`flagged_composition` accepts known defect slugs and a clamped limit. Unknown slugs add no
bits, and a request containing no known bit returns an empty queue. It returns ids only;
there is no `keep`, threshold, or delivery status.

Dismissal accepts one present defect. It refuses combinations, informational bits, and
absent notes. The updated reading is returned so the interface does not optimistically
invent a score or reason set that might disagree with the atomic store update.

## 7. Decision: analysis is cancellable and off the renderer thread

The client call is asynchronous. The native wrapper performs the synchronous project walk
outside the renderer thread, registers the supplied cancellation id, and removes it on all
normal completion paths. Finished rows are durable; the next call queries pending versions
and resumes rather than recomputing them.

Progress and aggregate composition/tilt telemetry contain counts, timings, scores, and
flag histograms. They contain no pixels, face geometry, filenames, or identity data.

## 8. Consequences

**Good.** The card cannot confuse absence with quality, cannot apply its own crop, and
cannot drift from backend semantics. Every low score is accompanied by inspectable evidence
or an explicit frame-wide reason.

**Bad.** Sending complete evidence makes the single-photo DTO larger than a score-only
surface, and the analysis command needs application services that the read commands do not.

**Ugly.** Automated component tests establish overlay coordinate and language behaviour,
but a real desktop-shell visual run and the 300-frame photographer audit remain exit
conditions until those environments are available.

## 9. Related

* `docs/adr/ADR-0023-composition-rules-and-aesthetics.md` - metric and persistence rules
* `crates/aura-app/src/composition_commands.rs` - native command boundary
* `ui/src/components/explain/CompositionCard.tsx` - accessible overlay and help text
* `docs/composition-and-framing.md` - public reason vocabulary

