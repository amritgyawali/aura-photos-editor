# Retouching behavior audit — 2026-09-30

**AURA is not an exact implementation of Retouch4me or SkinFiner.** This audit
compares publicly documented behavior with code paths and tests. We did not run
either competitor or obtain their output images, so no numerical match or
quality ranking has been established. There is no basis for calling AURA best.

| Capability | Documented reference behavior | AURA behavior after this change |
| --- | --- | --- |
| Skin targeting | Retouch4me Heal and SkinFiner describe automatic skin selection | Bundled YuNet detects face boxes and five landmarks. Auto portrait chooses a cheek/forehead sample and an editable mask; color similarity refines the selection. This is geometric skin targeting, not trained semantic skin segmentation |
| Natural smoothing | SkinFiner describes smoothing with texture preservation | New sample-guided smoothing separates fine detail from middle-scale variation and protects edges. Detail defaults to 100% |
| Uneven skin color | SkinFiner describes redness/yellow correction; Retouch4me Skin Tone describes skin-tone evening | New uniformity tool moves low-frequency chroma toward the chosen sample while preserving luminance. No automatic redness/yellowness classifier |
| Dodge and burn | Retouch4me documents automated portrait correction and optional Soft Light output | Sampled skin dodge/burn uses bounded exposure correction and preserves RGB proportions. Auto portrait targets detected faces with a restrained, editable correction; its strength policy is hand-authored, and Soft Light layer export is not implemented |
| Blemishes | Retouch4me Heal documents automatic problem-area detection | Local-statistics dark-spot detection plus texture-aware patch matching and approximate harmonic tone blending; no semantic distinction between temporary blemishes and permanent marks |
| Eye bags and facial features | SkinFiner describes eye-bag reduction on automatically detected faces | Automatic eye/nose/mouth landmarks guide protected areas. The under-eye correction tool still needs a manual selection |
| Presets | SkinFiner documents built-in and custom presets | Existing local named presets now include skin tolerance/edge settings and omit photo-specific samples/masks |
| Manual refinement | SkinFiner documents manual skin-mask refinement | AURA provides painted masks, erasure, pressure, feathering, numeric coordinates and sample picking |
| Non-destructive work | Retouch4me Dodge & Burn documents layer output | AURA saves editable operations and undoable recipe history. This is not layered PSD output |
| Precision/color | SkinFiner documents 16/32-bit processing and color management | Native processing uses f32 linear Rec.2020 and the existing color-managed export path; these additions were verified with 8-bit PNG delivery, not a new HDR/ICC certification |
| Batch portrait automation | Both products document batch or group workflows | Collection preparation now automatically detects suitable faces and saves editable texture, tone and light-balance steps. Repeat runs, manual protection, undo and PNG export were tested on five portraits; equivalent competitor quality has not been established |

The automatic-portrait follow-up is documented in
[ADR-0074](adr/ADR-0074-automatic-portrait-retouch.md) and the
[current validation report](automatic-portrait-validation.md). The earlier
sampled-skin verification below remains historical evidence for those processors.

## Sources

- [Retouch4me Heal](https://retouch4.me/heal)
- [Retouch4me Skin Tone](https://retouch4.me/skintone)
- [Retouch4me Dodge & Burn](https://retouch4.me/dodgeburn)
- [SkinFiner product features](https://www.photo-toolbox.com/product/skinfiner/)
- [SkinFiner user guide](https://www.photo-toolbox.com/product/skinfiner/user-guide/overview.html)

These describe advertised behavior, not disclosure of internal algorithms or
independent proof of visual quality.

## Verification and remaining evidence

The UI suite passed 541 tests across 57 files. Twenty-three native tests passed
(11 existing retouch tests extended to all 24 tools, seven sampled-skin tests,
and five patch-healing tests). TypeScript and production UI builds passed.

Strict Clippy checks were also run. They remain blocked by existing findings:
three in recipe `xmp.rs`/`schema.rs` (single-letter bindings and function length),
and 67 in render `retouch_tools.rs`/`retouch_mask.rs` (including bounded indexing).
The new `retouch_heal.rs` module produced no findings. Missing error documentation
in the touched recipe retouch API was fixed. This is not a clean repository lint
gate; no lint rules were relaxed to obtain a passing claim.

The native desktop build passed on this Windows machine using
`CARGO_INCREMENTAL=0` and the command-line override
`--config profile.dev.package.aura-vision.debug=0`, with binary flags
`-C debuginfo=0 -C link-arg=/DEBUG:NONE`. Two earlier link attempts reported a
corrupt `aura-vision` archive under Rust 1.97.1. Disabling that package's debug
information produced a structurally valid archive and completed the build.
This is a local build workaround, not an identified root-cause fix; release
profiles and repository build settings were not changed.
The patch-heal build later hit LNK1143 in an `aura-brain-photo` archive. Adding
`--config profile.dev.package.aura-brain-photo.debug=0` also rebuilt that package
and completed the desktop build. These remain command-line workarounds.

The sampled-skin tests measure luminance preservation, bounded exposure and chroma
preservation, unchanged unmatched colors, neutral settings, serialization/input
validation, fine-detail retention and reduction of a synthetic tonal pattern.
The same test pattern at several exposure levels checks signal-relative behavior;
it does **not** establish fairness across real skin tones. Edge tests exercise a
brightness boundary. Existing painted/erased-region isolation tests include all
24 tools.

The repeatable desktop workflow is `scripts/test-sampled-skin.py`. It uses five
real portrait JPEGs in a separate collection and verifies draft isolation,
actual pixel changes, original hashes, undo/redo and full-render/export equality.
Its measurements establish software behavior, not similarity to a competitor.

This run passed on all five portraits: all three tools changed preview pixels
without saving; saved edits survived undo/redo exactly; five PNGs passed export
read-back verification and matched the full renderer; and source hashes matched
the original downloads. The desktop sample controls and unsaved preview passed.
The test's error assertion was corrected to check the native error code rather
than specific English wording, then the workflow resumed from saved results.
Evidence is in `.work-checks/sampled-skin-review/results.json`, with full-size
before/after images and a desktop screenshot alongside it. Individual native
preview calls reported 878–2667 ms on this loaded development machine; this is
not a controlled throughput benchmark or a comparison against either product.

The patch-heal workflow also passed on the same five portraits in its own
collection (`scripts/test-sampled-skin.py --workflow patch-heal`). Automatic and
manual-source draft previews changed pixels without saving, saved repairs stayed
inside target bounds, undo/redo restored exact pixels, and five verified PNGs
matched the full renderer. Original hashes were unchanged. Desktop controls
correctly required a source for painted repairs, and before/after, discard and
return-to-Develop checks passed. Evidence is in
`.work-checks/patch-heal-review/results.json` with before/after images and a
workspace screenshot. These portraits verify integration; synthetic tests supply
the isolated blemish and texture-transfer evidence. Healthy skin test patches do
not establish real-acne removal quality or a comparison with commercial tools.

The subsequent comparison and draft-protection UI update passed 546 tests across
57 files, TypeScript/Vite and the Windows desktop build. The aligned split view
uses cached native previews and supports keyboard/pointer inspection at shared
zoom and pan. `scripts/test-retouch-comparison.py` passed on all five portraits,
including unchanged recipes, history and original hashes. It also checked an
unsaved refinement and Discard on the first portrait. This adds review controls
and protects in-memory drafts; it does not change the retouch algorithms or
establish competitor parity. See [ADR-0072](adr/ADR-0072-retouch-comparison-and-draft-protection.md).

The advanced selection update adds gradients, shape inversion, brightness ranges
and a disposable coverage preview across all 24 tools. The preview evaluates
the range at the operation's position in the saved stack; later operations,
sharpening and final geometry cannot alter its selection. It displays authored
coverage, before tool-specific skin affinity or spot detection. These capabilities
improve manual targeting; they do not close the automatic semantic-selection gap.
See [ADR-0073](adr/ADR-0073-advanced-retouch-selections.md).

Verification includes 551 UI tests across 57 files and 29 focused native tests.
The last endpoint-guide correction also passed the 21 affected UI tests and
TypeScript/Vite. Strict Clippy was rerun for the recipe and render libraries:
the same 3 recipe and 67 renderer baseline errors remain. Error-category counts
match the prior patch-heal run; no new categories or additional findings were
introduced. The new bounded RGB indexing has a local, justified lint allowance;
existing findings were not suppressed. Logs are in
`.work-checks/selection-recipe-clippy.log` and `selection-render-clippy.log`.

The final Windows desktop build and the five-portrait selection workflow passed.
All five PNGs passed verification and matched the full renderer exactly; protected
pixels and original download hashes were unchanged, undo/redo was exact, and
actual gradient/range/mask controls passed on all five portraits. One desktop
process exit interrupted the first run on the fifth portrait, with no cause in
the checked logs. After restart, completed operation IDs and pixels were intact;
the resumed run completed all remaining checks. The exit remains unexplained.
Evidence: `.work-checks/selection-review/results.json`. The repeatable script is
`scripts/test-retouch-selection.py`, including `--resume` for interrupted runs.

To establish closer parity, a future evaluation needs paired originals and
competitor outputs with versions/settings recorded, face/skin reference masks,
diverse lighting and complexions, and blind review of pores, hair, eyes, lips,
permanent marks, color shifts and halos at full resolution. Pixel difference
alone cannot establish the better retouch. Automatic semantic selection,
permanent-feature protection and trained repair remain separate gaps.
