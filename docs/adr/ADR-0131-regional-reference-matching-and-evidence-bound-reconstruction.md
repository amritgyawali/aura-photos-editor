# ADR-0131: Regional reference matching and evidence-bound reconstruction

Date: 2026-10-20. Extends ADR-0130.

## Context

The requested workflow needs separate reference-photo colour strengths for skin,
hair, clothing and background. A global colour fit with region-labelled sliders
would not provide that behaviour. Installed Evoto artifacts also distinguish
segmentation, task-specific inference, protection masks and composition; those
observations do not provide its trained networks or establish identical output.

## Decision

Use AURA's existing pinned, licensed semantic segmenter and native local-mask
renderer. Measure the shared face-skin, body-skin, hair, clothing and background
classes in the target and reference. Pool people by semantic class; do not infer
person-to-person correspondence. Freeze bounded 256-by-256 selection mattes for
the session. Measure the rendered crop while keeping selections in full-frame
coordinates. Missing or unsuitable regions have explicit reasons and no invented
correction.

Fit nine supported local parameters: exposure, contrast, highlights, shadows,
whites, blacks, temperature, tint and saturation. Store independent amount, tone,
colour and white-balance multipliers for each represented region. Always rebuild
from the immutable saved baseline, preserving manual masks and unrelated recipe
fields. Zero restores that baseline, including absent extension keys. Derive mask
identities from content and engine versions, rather than random session IDs.

Before committing, render at native resolution and check the existing colour,
highlight and regional-distance safeguards. Refuse stale recipe hashes and invalid
session metadata. Save the candidate recipe and its history atomically as one
edit, then persist the session metadata in the catalogue. If that separate
metadata write fails, surface the error; the saved edit and Undo remain intact,
but adjustment can require a fresh analysis. The two writes are not a single
transaction.
Removing a library reference does not invalidate an already saved matching session.
Changing another saved edit requires a fresh analysis.

Organize the editor into seven selectable panels while keeping controls mounted
to preserve unsaved drafts. Stop drawing, picking and warping gestures when their
panel is hidden. Keep the preview beside controls on wide windows and retain
common Undo/Redo controls.

## Evidence boundary

Keep installed-app research separate from shipped AURA assets. Record binary
hashes, REA evidence IDs, bounded static traces and unresolved questions. Metadata
auditing may report effect names, schema keys, parameter ranges and resource
references; it must not write decoded competitor images, shader bodies or weights.
Independent AURA code uses its own renderer and licensed models. No recovered
Evoto code, tuning arrays or trained assets enter the product.

## Consequences and validation

Regional strengths now cause actual local changes. They do not reproduce Evoto's
Quick/Control modes, per-person matching, local curves/HSL, learned skin cleanup
or expression/body generation. Exact output parity requires paired photographic
evaluation and remains unestablished.

Native tests exercise masked pixels, zero restoration, crop coordinates, manual
masks, deterministic recipes, history, reopening, stale writes and validation.
UI tests exercise independent strengths, unavailable regions, failed writes,
cancellation and panel navigation. No frozen contract, schema migration, new
model weights or image upload is introduced.
