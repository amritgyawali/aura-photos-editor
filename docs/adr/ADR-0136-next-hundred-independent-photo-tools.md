# ADR 0136: Independent photo tools and bounded inspection

Status: accepted for local implementation; photographic parity remains unestablished.

The request adds another 100 capabilities after the preceding hundred. Evoto's installed
files are evidence to inventory and inspect, not a source of licensed model weights or
proof of recovered processing. A complete read-only file manifest and REA resource-table
evidence record exactly what was examined, including excluded historical installer depth.

Extend the existing validated `color_lab_v1` and `spatial_lab_v1` enums with 25 independent
methods each, retaining their 32-operation stack bounds, explicit selections, revision checks,
history and manual protection. Add 25 explicit selection workflows and 25 read-only working
proxy measurements. No frozen IPC shape or recipe migration changes. New inspection groups
share one saved-revision report and remain available through three additional editor tabs.

All new methods are original implementations of stated mathematics. No Evoto assets,
weights, shaders, training data, or executable code are shipped. Processing stays offline;
original photos stay read-only. Spatial methods retain the existing bounded correction grid,
so they do not claim sensor-scale reconstruction. Diagnostics describe the rendered proxy,
not trained face/body inference or a universal quality verdict. The render cache identity
advances to `labs.selected-v3` so old pixels cannot satisfy new processing requests.

Validation must include native mathematical fixtures, selected-pixel isolation, deterministic
neutral behaviour, revision and Undo/Redo checks, visible panels, and separately verified
edits/exports of the explicitly supplied RAW folder. Report failures and scope limits rather
than treating a tool count as Evoto equivalence or human preference evidence.
