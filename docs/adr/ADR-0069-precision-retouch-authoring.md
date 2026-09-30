# ADR-0069 — Precision retouch authoring

- Date: 2026-09-30
- Status: accepted

Extend the native workspace from ADR-0068 with reusable precision controls across
its 20 operations. This is a refinement of the current editor, not a new product
phase or a claim that every editing feature has been implemented.

An optional `mask` field carries ordered normalized brush strokes. Omission keeps
the previous ellipse behavior and serialization. Strokes have add/erase mode,
radius relative to the short image edge, opacity, and normalized x/y/pressure
points. Each stroke forms a continuous swept disk; pressure changes radius.
Add uses maximum coverage and erase subtracts coverage once per stroke, so event
frequency does not multiply opacity. Point and stroke counts are bounded.
The same Rust rasterizer controls every native operation and export, including
automatic spot repair. Masks remain anchored before crop/perspective.

Draft previews validate and render a temporary recipe without saving history.
The UI debounces, serializes preview requests and discards outdated results.
Applying a draft creates one normal history entry. Reordering and duplication
also go through ordinary recipe merge/history, preserving manual ownership.

Zoom and pan are view transforms only. Pointer coordinates use the transformed
image rectangle; keyboard controls and numeric coordinates remain alternatives.
Saved presets contain tool settings, not donor points, masks or photo identifiers,
and use a bounded, versioned local preference store with visible error handling.

No new image model, network service or dependency is introduced. These controls
do not establish automatic face recognition, generative reconstruction, layered
documents or commercial quality parity. Whole-frame rendering remains a memory
limitation. Validation covers old recipes, brush geometry/erasure, mask isolation,
preview non-persistence, history and native export on real photographs.
