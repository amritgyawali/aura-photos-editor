# ADR-0096: Complexion-faithful automatic retouch

Status: Accepted
Date: 2026-10-07

## Context

The user wants new photographs edited automatically while retaining their real skin.
The adaptive planner already samples each person and stores reversible operations, but
absolute skin luminance could be labelled deep shadow, low-signal samples received weaker
corrections purely because they were darker, and even skin received a minimum colour
correction. A histogram highlight lift could also override the dark-background protection.
In the native acne fixture, the dense-pattern guard rejected every proposed spot repair.

## Decision

- Compare sample variation relative to its own signal, select a representative patch by
  relative texture and median colour, and stop classifying skin brightness as shadow.
  Noise, resolution and directional lighting continue to provide evidence for restraint.
- Reduce colour correction to zero on measured even skin. Reduce colour as well as light
  evening under directional light so warm/cool shading is not flattened into one colour.
- A subject at least as bright as its surroundings retains the existing +0.25 EV ceiling;
  a missing white highlight cannot override it. A detected face alone no longer raises a
  dark frame toward 18% grey. Clipping protection remains. Histograms cannot establish
  the intended exposure of every portrait; exposure is still editable.
- New Natural settings retain 85% on the pore control (91% finest-band retention in the
  smoothing operation) and leave body-to-face colour matching off. Explicit saved choices
  and other presets remain authoritative. No change to renderer semantics or old recipes.
- A dense mark pattern keeps weak departures. The whole-face search may propose compact
  marks whose normalized redness exceeds their local surrounding skin by at least 0.025.
  Darkness alone cannot grant this exception. Every proposal remains masked, feature
  protected, donor checked, bounded by the spot limit, explained, and undoable. This is a
  conservative heuristic, not a diagnosis or proof that a mark is temporary.

- Exclude every detected face from all automatic body selections, including sampled-colour
  fallback and body-only passes. A hand beside a face can otherwise leak a body selection
  into an eyelid. Protected holes retain a cell margin and disable further edge refinement;
  the existing selection, sample, body contours and manual operations remain authoritative.

Planner versions become `sample-consensus-v5`, `measured-features-v5` and
`expert-adaptive-v2-skin-fidelity`. Existing `adaptive` and portrait disable controls remain.
No IPC shape, catalog migration, model, image upload service, or export contract changes.

## Validation and limits

`scripts/verify-skin-fidelity.py` runs real native import, one-click editing, retouch-only
comparisons, repeat passes, Reset/Undo/Redo, optional full-size verified PNG export and
original-file hashes. Measurements compare retouch with the same photograph before
retouch so exposure/white balance are not mistaken for skin correction. It also saves
face-detail pairs for visual inspection. Unit regressions cover equal decisions across
complexion luminances, relative sampling, even skin, directional light, dark backgrounds
and mixed dense marks. UI preferences retain explicit older choices.

Small, occluded or incorrectly segmented faces and ambiguous pigmented marks still require
review. No universal skin accuracy or perfect automatic retouch claim is established.
