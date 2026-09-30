# Retouching behavior audit — 2026-09-30

**AURA is not an exact implementation of Retouch4me or SkinFiner.** This audit
compares publicly documented behavior with code paths and tests. We did not run
either competitor or obtain their output images, so no numerical match or
quality ranking has been established. There is no basis for calling AURA best.

| Capability | Documented reference behavior | AURA behavior after this change |
| --- | --- | --- |
| Skin targeting | Retouch4me Heal and SkinFiner describe automatic skin selection | A photographer samples a skin patch; a color range intersects a painted/ellipse selection. No learned skin/face segmentation in the native workspace |
| Natural smoothing | SkinFiner describes smoothing with texture preservation | New sample-guided smoothing separates fine detail from middle-scale variation and protects edges. Detail defaults to 100% |
| Uneven skin color | SkinFiner describes redness/yellow correction; Retouch4me Skin Tone describes skin-tone evening | New uniformity tool moves low-frequency chroma toward the chosen sample while preserving luminance. No automatic redness/yellowness classifier |
| Dodge and burn | Retouch4me documents automated portrait correction and optional Soft Light output | New sampled skin dodge/burn uses bounded exposure correction and preserves RGB proportions. No face-aware learned decision or Soft Light layer export |
| Blemishes | Retouch4me Heal documents automatic problem-area detection | Existing local-statistics dark-spot detection and sampled healing; no semantic distinction between temporary blemishes and permanent marks |
| Eye bags and facial features | SkinFiner describes eye-bag reduction on automatically detected faces | Existing under-eye tool needs manual selection; no automatic anatomical localization in this workspace |
| Presets | SkinFiner documents built-in and custom presets | Existing local named presets now include skin tolerance/edge settings and omit photo-specific samples/masks |
| Manual refinement | SkinFiner documents manual skin-mask refinement | AURA provides painted masks, erasure, pressure, feathering, numeric coordinates and sample picking |
| Non-destructive work | Retouch4me Dodge & Burn documents layer output | AURA saves editable operations and undoable recipe history. This is not layered PSD output |
| Precision/color | SkinFiner documents 16/32-bit processing and color management | Native processing uses f32 linear Rec.2020 and the existing color-managed export path; these additions were verified with 8-bit PNG delivery, not a new HDR/ICC certification |
| Batch portrait automation | Both products document batch or group workflows | Native operations are authored per photograph. AURA's collection automation is not validated as equivalent face-aware retouching |

## Sources

- [Retouch4me Heal](https://retouch4.me/heal)
- [Retouch4me Skin Tone](https://retouch4.me/skintone)
- [Retouch4me Dodge & Burn](https://retouch4.me/dodgeburn)
- [SkinFiner product features](https://www.photo-toolbox.com/product/skinfiner/)
- [SkinFiner user guide](https://www.photo-toolbox.com/product/skinfiner/user-guide/overview.html)

These describe advertised behavior, not disclosure of internal algorithms or
independent proof of visual quality.

## Verification and remaining evidence

The UI suite passed 540 tests across 57 files. Eighteen native tests passed
(11 existing retouch tests extended to all 23 tools, plus seven sampled-skin
tests). TypeScript and production UI builds passed.

The native desktop build passed on this Windows machine using
`CARGO_INCREMENTAL=0` and the command-line override
`--config profile.dev.package.aura-vision.debug=0`, with binary flags
`-C debuginfo=0 -C link-arg=/DEBUG:NONE`. Two earlier link attempts reported a
corrupt `aura-vision` archive under Rust 1.97.1. Disabling that package's debug
information produced a structurally valid archive and completed the build.
This is a local build workaround, not an identified root-cause fix; release
profiles and repository build settings were not changed.

The sampled-skin tests measure luminance preservation, bounded exposure and chroma
preservation, unchanged unmatched colors, neutral settings, serialization/input
validation, fine-detail retention and reduction of a synthetic tonal pattern.
The same test pattern at several exposure levels checks signal-relative behavior;
it does **not** establish fairness across real skin tones. Edge tests exercise a
brightness boundary. Existing painted/erased-region isolation tests include all
23 tools.

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

To establish closer parity, a future evaluation needs paired originals and
competitor outputs with versions/settings recorded, face/skin reference masks,
diverse lighting and complexions, and blind review of pores, hair, eyes, lips,
permanent marks, color shifts and halos at full resolution. Pixel difference
alone cannot establish the better retouch. Automatic semantic selection,
permanent-feature protection and trained repair remain separate gaps.
