# Evoto material controls: bounded evidence and independent AURA extensions

Date: 2026-10-20. This continues the [native research](AURA-Evoto-native-research-2026-10-20.md) and [workflow audit](AURA-Evoto-workflows-2026-10-20.md).

## Result and scope

A fresh REA MCP pass inspected the installed GUI and worker PE resources. A focused static pass then examined material-related strings, verified selected resource identities, and decoded six native ranges using the existing GNU objdump installation. This adds evidence for hair smoothing, white-hair darkening, backdrop banding controls, and a separate clothing-moire request/result workflow.

This is partial static reverse engineering. It does not recover Evoto's trained models, training data, proprietary editing algorithms, full processing graph, or original source. There was no fresh photographic editing/export comparison or runtime activation of these effects. Exact image equivalence and best-in-class quality remain unqualified.

No competitor code, shaders, texture assets, tuning arrays, or model weights were copied into AURA. The implementation concepts below describe independent AURA methods. They must not be represented as recovered Evoto internals.

## Target-bound REA evidence

The explicitly requested [reverse-engineer-anything skill](C:/Users/Admin/.agents/skills/reverse-engineer-anything/SKILL.md) was applied through the connected REA MCP tools. Both resource inspections were target-free operations; this pass opened no active REA binary target and therefore had no target lifecycle to close. No MCP/client configuration changes or tool installation were performed.

| Artifact | Bytes | SHA-256 | Fresh REA evidence ID |
| --- | ---: | --- | --- |
| `F:/evoto/Evoto-worker.exe` | 230489488 | `add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2` | `ev_c06192a95f52887d5f461d2a0abdfb88dbb565de353627d7d87800856a1f6c30` |
| `F:/evoto/Evoto.exe` | 237784976 | `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa` | `ev_0aaab3b16b3d6b277303b9302dc151f4166d4f85cc5b158dbb0da34aa346c25b` |

Each resource tree contains four payloads across twelve examined entries. The REA coverage is complete for those bounded PE resource trees, not the application. Icon, group-icon, version and manifest metadata do not explain photo processing. Full unabridged MCP results are retained locally in `.work-checks/evoto-next/worker-pe-resources.json` and `gui-pe-resources.json`.

The parent investigation observed that the current REA Ghidra provider lacked usable environment configuration. This pass continued with supported target-free REA inspection and the existing native disassembler; it did not rerun the prior expensive Ghidra operation or claim a successful fresh Ghidra dossier.

## Fresh static material findings

The existing [bounded PE scanner](../../scripts/evoto-static-trace.py) was reused through a local wrapper with a material-specific pattern. The wrapper retained 134 matching ASCII strings, 103 with candidate associations, and 136 RIP-relative LEA encoding candidates. It generated 65 bounded candidate unwind ranges. These counts are limited to the inspected data sections and selected pattern; they are not function or feature completeness counts. Unwind ranges can be source-function fragments.

The existing GNU objdump 2.47.20260726 installation decoded six selected ranges. All eight selected LEA associations were confirmed against decoded absolute string targets. Complete disassembly and tool identity are retained in `.work-checks/evoto-next/native-material-links.json` and its six named text artifacts. No new decompiler output was generated.

| Range start | Observed decoded association | Supported inference | Unknown |
| --- | --- | --- | --- |
| `0x144b90b00` | Hair-smoothing empty-hair-mask diagnostic at string address `0x14adff468` | The hair-smoothing path has a hair-mask input requirement | Direction estimation, filter kernel, task model and photographic result |
| `0x144bf9390`, `0x144c18a00` | White-hair effect empty-hair-mask diagnostic at `0x14ae056d8` | Separate white-hair handling paths depend on hair masks | Gray-strand classifier, highlight preservation, composition order |
| `0x144bb2d90` | Backdrop de-moire processor-creation failure diagnostic at `0x14ae01190` | A separate backdrop processor wrapper exists | Whether its implementation removes luminance, colour, quantization steps, or another artifact |
| `0x14572e3c0` | Clothing-moire ROI-upload and cache-invalidation diagnostics | An upload-related clothing-moire wrapper is shipped | Runtime network activity, actual service, authentication, request content and successful output |
| `0x146c10630` | Clothing-moire request-seed empty-ROI-path and transparent-source diagnostics | Request preparation has explicit input guards | Complete request protocol, remote algorithm, availability and trained model |

The request/upload/result/download/history string family suggests that at least one clothing-moire workflow uses a service-style or generated-result architecture. That is an inference from static diagnostics, not proof that a particular operation sent an image or that every clothing-moire path is remote. No endpoint was fetched and no account or private project storage was inspected.

The worker also contains the user-facing label **Color Banding Removal** and an embedded description limiting the operation to solid backgrounds. These are shipped declarations, not a runtime measurement. The word “banding” also appears in an unrelated Ribbon-filter cache diagnostic; that occurrence is explicitly excluded from photo-effect conclusions.

## Resource and earlier UI cross-checks

Eighteen material-related configuration records were selected from the preceding metadata-only packaging audit. Every current configuration file hash matches its retained record. The compact fresh check is `.work-checks/evoto-next/material-resource-identity-checks.json`. No decoded shader source, tuning arrays, images or model payloads were retained.

Relevant declared categories include `HairSmooth`, `RemoveWhiteHair`, `HairSeam`, `HairGloss`, `HairColorAdjust`, `HairColorUnify`, `ClothesCrease`, `ClothesBlemishRemoval`, `ClothesEdgeSmooth`, `ClothRemoveSweat` and `BgDeMoire`. Metadata labels establish declared categories, not their numerical behavior.

Earlier same-day accessibility snapshots, clearly retained as historical reference rather than fresh runtime observation, expose:

- Hair: Smooth Hair; White Hair Blackening.
- Clothing: Fine Wrinkles Removal; Deep Wrinkles Removal; Clothing Edge Smoothing.
- Backdrop: Color Banding Removal; Solid Backdrop Refinement.

Their source paths, digests and matching labels are retained in `.work-checks/evoto-next/historical-material-ui-matches.json`. Hair **direction controls** and a specific **directional luminance-notch algorithm** were not established by those snapshots or the static metadata. Those are independent AURA design choices.

## Fifteen independent material and restoration concepts

These are implementation requirements for this continuation, not a claim that Evoto has these fifteen exact algorithms. “New” means an additional explicit AURA operation or control path; it does not mean that frequency filtering, denoising or retouching did not already exist in AURA.

| # | AURA concept | Independent method and distinction from existing AURA | Evoto provenance |
| ---: | --- | --- | --- |
| 1 | Directional hair smoothing | Smooth along an explicitly chosen strand direction inside authored coverage; retain transverse structure and allow exact zero | Hair-smoothing category observed; directional method not recovered |
| 2 | Silver/white hair darkening | Gate selected bright achromatic areas by their local mean; retain residual detail and guard local brighter highlights | White-hair category observed; AURA uses a brightness/chroma heuristic, not a strand classifier |
| 3 | Fine/coarse fabric balancing | Adjust separate medium/fine frequency bands with separate strengths inside garment coverage | Fine/Deep wrinkle controls observed historically; not recovered filter math |
| 4 | Directional luminance moire suppression | Attenuate a selected oscillation direction/scale in luminance while retaining chroma and broad garment form | Clothing-moire category observed; notch method is an AURA design |
| 5 | Flat-region debanding | Gate local smoothing to low-gradient selected backdrop regions, retaining edges and real texture | Solid-background banding category and limitation declared; algorithm unknown |
| 6 | Median chroma cleanup | Robust neighborhood chroma median at fixed photographed luminance inside coverage | AURA extension; specific Evoto equivalent not established |
| 7 | Isolated impulse repair | Detect center outliers against robust neighbor agreement and replace only isolated anomalies | AURA extension; specific Evoto equivalent not established |
| 8 | Thin line-scratch suppression | Compare orthogonal neighborhood supports, repair thin anomalies while guarding stronger edges | AURA extension; specific Evoto equivalent not established |
| 9 | Row/column stripe correction | Fit selected residual offsets along one chosen image axis; retain broader illumination | AURA extension; specific Evoto equivalent not established |
| 10 | Checkerboard artifact suppression | Estimate alternating grid-phase residuals and attenuate the matching local artifact | AURA extension; specific Evoto equivalent not established |
| 11 | Edge-ringing repair | Suppress existing local overshoot/undershoot around photographed edges with explicit authored strength | AURA extension; specific Evoto equivalent not established |
| 12 | Specular chroma reconstruction | Borrow reliable neighboring chroma into selected near-neutral bright defects without inventing highlight texture | AURA extension; specific Evoto equivalent not established |
| 13 | Broad chroma mottle correction | Attenuate coarse colour variation separately from fine luminance texture | AURA extension; specific Evoto equivalent not established |
| 14 | Planar backdrop illumination leveling | Fit a selected linear-luminance plane and remove its bounded slope | AURA extension; broad backdrop category exists but this method is not recovered |
| 15 | Cross-channel detail reconstruction | Use a reliable channel's local detail to repair selected damaged-channel detail with bounded gain | AURA extension; specific Evoto equivalent not established |

### Existing-feature overlap that implementation must preserve

AURA already exposes `Fabric`, `Frequency`, `Wrinkle`, `TextureGraft`, `FabricMoire`, `FabricStain`, `ShadowBalance`, `Backdrop`, hair colour, hair detail, hair shine, sampled colour and authored masks. The [current retouch recipe](../../crates/aura-recipe/src/retouch_tools.rs), [material renderer](../../crates/aura-render/src/retouch_material.rs), [retouch renderer](../../crates/aura-render/src/retouch_tools.rs), and [restoration stages](../../crates/aura-render/src/restore.rs) establish those existing mechanisms.

The existing `FabricMoire` operates on chroma while preserving luminance. A new luminance operator therefore addresses a different residual. Existing `Fabric` already changes medium/fine structure through tone and texture controls; independently adjustable coarse/fine bands expand that control, and must not be counted as the first frequency-separation implementation.

The existing restoration pipeline already separates chroma/luminance denoising and measures ringing to govern deconvolution. Explicit median cleanup and existing-ringing repair are additional selected methods, not the introduction of denoising or anti-ringing safeguards. Existing generic Burn/Colorize can darken hair; AURA's dedicated operation additionally gates brightness and chroma inside the user's selection, without claiming individual-strand detection. Existing backdrop and shadow tools overlap broad illumination correction; the plane-fit operator provides a distinct bounded slope fit.

## Acceptance and remaining unknowns

Each independent image operator should demonstrate a nonzero useful pixel change on an appropriate fixture; exact zero; unchanged excluded pixels; deterministic repeated rendering; fractional-mask behavior; finite output on dark/HDR and degenerate inputs; and bounded memory/time at supported image sizes. Existing recipe/history, persistence, undo/redo, preview/export, cancellation and original-file protections must remain applicable.

Synthetic tests can verify those invariants and signal responses. They do not establish natural photographic quality on diverse hair, silver highlights, veils, skin tones, patterned fabrics, lace, textured walls or multilight backdrops. Fixed, permissioned real-image cohorts and paired native-resolution outputs remain necessary before any equivalence claim.

This report records evidence and implementation distinctions. Product integration and its actual test results belong in the implementation report; this evidence pass alone does not mark the fifteen concepts or the user's requested fifty additions complete.

Full local evidence and helper scripts reside under `.work-checks/evoto-next/`. They are investigation artifacts and are excluded from product source. The current pass did not execute the competitor, activate a trial, invoke paid or remote editing, export a photograph, or copy trained assets.

