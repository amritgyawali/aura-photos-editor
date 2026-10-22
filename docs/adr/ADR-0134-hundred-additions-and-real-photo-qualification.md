# ADR-0134: One hundred additions and supplied-photo qualification

Date: 2026-10-21. Continues ADR-0133. The user requests one hundred more unique
capabilities, use of the installed Evoto app and REA evidence, and actual edits
and tests of the 27 Canon CR3 photographs in `H:\AURA EDIT`.

Implement four distinct scopes: 25 explicit colour methods, 25 spatial repair
methods, 25 selection authoring/refinement workflows, and 25 durable review and
batch-edit workflows. Count complete operations, not sliders, reset buttons,
colour channels or newly named panels. Existing capabilities from both prior
fifty-item reports are excluded. Methods use original implementations;
proprietary source/weights/training data and exact photographic equivalence are
not established by diagnostic strings, decompilation or preview observation.

The colour and spatial stacks use validated, neutral-on-old-recipe extension
keys outside frozen contracts. They require explicit mask IDs, honour mask
enabled/opacity state, preserve unselected pixels, and use one stale-checked
history save. They share native preview/export ordering and full-frame fallback;
later-stage keys are excluded from pre-retouch checkpoints and included in render
hash/invalidation. Colour-only comparison omits spatial corrections. Spatial
methods use a bounded 256-pixel grid with interpolated correction rather than
unbounded nonlocal work on 45 MP originals. This is explicit in UI and docs.

Selection methods retain the existing bounded matte encoding and history API;
measured selectors identify their source-preview domain. The review workspace
stores revision-bound metadata in atomic catalogue transactions, validates
portable interchange, and clears imported approvals. Prepared batch recipe
plans preflight all source/target revisions then use per-photo compare-and-save
and Undo; the UI reports individual outcomes instead of asserting a cross-photo
all-or-nothing transaction. Review labels never send external messages.

Use isolated catalogues and WebView profiles for qualification. Import the
supplied originals read-only, hash them before and after, save before/after
previews and recipe JSON, then verify full-resolution JPEG deliveries through
AURA and an independent image decoder. Test arbitrary methods on explicit
fixtures or reversible copies; do not apply destructive artistic/inpainting
methods blindly to every portrait and call that an improvement.

Run full workspace/UI tests, strict lint, contracts and IPC uniqueness checks,
rebuild and launch the production-asset desktop, and record individual runtime
case outcomes. Broad tests cannot certify every platform, control combination,
network provider, physical device or photographic quality criterion. Report
pass/fail/conditional/unavailable/untested explicitly. Compare actual Evoto
settings/preview only within the available evidence; do not activate trials,
purchase credits, or describe blocked exports as obtained reference images.
