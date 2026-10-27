# AURA workflow improvements — 19 October 2026

This implementation addresses the actionable import, numeric-input and large
retouch-history findings in the full-feature audit. It is a tested workflow
upgrade, not an exact Evoto clone or a new photographic-quality certification.
The original audit reports remain unchanged.

## Review and scope

Reviewed the main report, function inventory, machine-readable report and its
HTML/CSV companions: 325 native commands, 257 desktop cases, 4,265 native test
records and 788 UI test records. Those are historical audit counts, not the
number of tests rerun for this change. Traced the affected Rust import/catalogue
paths, background jobs, native bindings, React editor and retouch-operation UI.

Evoto's public [batch workflow](https://support.evoto.ai/save-time-with-batch-style-matching-sync-presets-in-evoto/)
and [portrait workflow](https://support.evoto.ai/portrait-retouching-feature-module/)
were used as capability references. This work does not use Evoto's internal
implementation, model weights or proprietary assets.

## Implemented

### Imports and automatic workflow

The failing regression reproduced a catalogue uniqueness error on a duplicate
individually selected file. Such selections previously used an empty relative
path. They now use the containing directory and filename, which also allows a
subsequent folder import to reuse the same source root.

- Mixed selections containing a duplicate, invalid metadata-only JPEG, empty
  file and later valid roots continue through the tested scenario. Empty files
  are quarantined; metadata-only import does not certify that a file can decode.
- Selecting a single file does not mark unselected siblings absent.
- Discovery covers the complete selection before progress obtains its total.
  Progress includes fast skips and rejections and stays cumulative across roots.
- Background progress now reaches both the native event stream and polling.
- Inserts and their counters commit in one transaction. Terminal journal state,
  final counters, completion timestamp and error code commit together.
- Terminal errors survive repeated polling. The running flag and error are read
  together, avoiding a race that could falsely report successful completion.
- Failed worker startup releases job state. Import warnings stop the UI's automatic
  handoff, and a dependent one-click run refuses to export after an import failure.

### Editing and retouch review

- Whole-number inputs reject fractional values locally with a field-specific,
  accessible error. Valid correction and Escape work without preview retry.
- Decimal controls retain precise typed values, including sharpening radius
  `1.375`; slider increments do not unnecessarily quantize these inputs.
- Edit errors are separate from preview failures. The editor reloads authoritative
  recipe/history after a failed write and permits another attempt, including the
  same value. The status text does not claim that a failed edit was saved.
- Saved retouch operations have search, enabled/disabled filters and pages of 24
  rows. A selected operation can be located directly. All actions retain stable
  operation IDs and absolute stack positions, including reorder, duplicate,
  remove and strength adjustments. Paging does not truncate the saved recipe.
- Removed the duplicate `within_moment` registration. The shell inventory now
  contains 325 names with no duplicate registrations.

Implementation decision and rollback:
[ADR-0126](../adr/ADR-0126-import-recovery-and-responsive-retouch-review.md).

## Validation

| Check | Result | Scope |
|---|---|---|
| Complete UI suite, one worker | 794 passed in 95 files | Includes six added regressions; original timeout unchanged |
| Focused editor and retouch UI suite | 59 passed | Validation, failed-save recovery, paging, filtering and stable actions |
| Ingest crate | 28 passed | Includes duplicate mixed roots, reimport, monotonic progress, failure/cancel journal closure and surviving committed files |
| Background import integration | 2 passed | Actual worker, events, polling, original-byte preservation and failed-import delivery refusal |
| Existing one-click delivery integration | 1 passed | Offline delivery/export and cancellation |
| Existing photo workflow integration | 2 passed | Editing, manual protection, original preservation and export |
| Existing colour-reference integration | 1 passed; 1 ignored | Ignored RAW case requires external input paths |
| Frontend production build | Passed | TypeScript and Vite; existing bundle-size warning remains |
| Bundled native desktop build | Passed | Debug executable with `custom-protocol`; linker emitted a multiple-manifest resource warning |
| Rebuilt desktop regression checks | 4 passed | Original six-root import, numeric correction/Undo, 300-operation paging/search and source-byte preservation |
| Frozen contracts | Passed | All 83 entries locked; no wire-type relock |
| Strict ingest Clippy, all targets | Passed | Warnings treated as errors |
| Strict application Clippy | Not clean | Existing issues in colour matching and portrait code; see below |
| Banned-pattern check | Not clean | Existing `.expect()` in `portrait_features.rs` bundled profile loader |

The updated 300-operation UI regression completed in 365 ms in the focused run.
It now finds its target through search, so this is not a controlled before/after
benchmark. The bounded-row test also exercises a 513-operation list, its last
operation, filters and page shrinkage. No claim is made about renderer throughput,
large-wedding memory or physical UI latency from these component tests.

The rebuilt desktop executable was also launched against a fresh, isolated test
catalogue. The original six source paths (including a duplicate PNG, corrupt JPEG,
real portrait JPEG and Canon CR3) reached `done=6`, `total=6`, `running=false`.
The journal recorded `completed`, six file rows, one duplicate and a completion
timestamp; the collection contained five distinct photographs. This verifies
import completion, not decoding of the corrupt JPEG or full RAW qualification.

In the actual WebView UI, entering sharpening `22.5` showed the local error while
leaving other controls enabled and the recipe unchanged. Correcting it to `23`
saved successfully; Undo restored the original recipe hash. A saved stack of 300
zero-strength clone operations mounted 24 rows, paged to the next set and found
the final operation by ID with its action enabled. All six source SHA-256 hashes
matched before and after. Zero-strength operations test stack interaction, not
retouch rendering quality. The first harness attempt omitted collection selection;
explicitly selecting the test collection resolved that navigation timeout.

Desktop evidence and screenshots are in
`.work-checks/workflow-improvements-2026-10-19/run2/`. The rebuilt executable is
`target/desktop-launch/debug/aura-desktop.exe`, available through `Start AURA.cmd`
in this checkout. No installed application copy was present or replaced.

Strict application Clippy identified existing findings in
`color_match/appearance.rs`, `color_match/reference_image.rs`, `color_match.rs`,
`portrait_features/jaw_guard.rs` and `portrait_features.rs`: manual containment,
similar names, oversized functions, a placeholder variable name, manual clamp and
missing panic documentation. These checks are not represented as passing. The
bundled profile loader also fails the repository's banned-pattern script.

Logs are under `.work-checks/` with the `aura-improvements-` prefix: `ui-full.log`,
`ui.log`, `ingest-final.log`, `ingest-worker-final.log`, `app-tests.log`,
`app-final.log`, `build-final.log`, `ingest-clippy.log`, `app-clippy.log`,
`contracts.log`, `banned.log`, `desktop-build.log` and `desktop-verification.log`.

## Remaining work toward commercial parity

| Area | What remains unproven |
|---|---|
| Portrait rendering | Native-scale skin texture, anatomical edges, makeup, facial hair, occlusion and varied skin tones against representative photographs |
| Colour and style | Perceptual equivalence to a target editor, calibrated ICC/print output and robust mixed-camera/mixed-light matches |
| RAW support | Qualification with real files from each supported camera and compression variant |
| Scale and hardware | Release-build interaction, memory and throughput on large weddings and multiple GPU families |
| External workflows | Live provider, upload, billing/activation, physical tablet and trained personal-style acceptance |
| Import edge cases | Arbitrary source aliases, overlapping roots and every filesystem failure mode; the tested duplicate-alias case is narrower |

No retouch algorithm or model was changed in this implementation. Existing tests
and report entries are not evidence of exact Evoto output. A full product-parity
claim requires a defined feature matrix, representative reference outputs and
independent image-quality evaluation.
