# AURA: 100-feature photo editor roadmap

Research date: 30 September 2026. AURA baseline: `a69b236`.

Implementation has started after this baseline. See the [current implementation report](photo-editor-implementation.md) for shipped changes, checks and remaining work; the baseline statuses below are preserved for comparison.

This is a prioritized catalog of 100 distinct feature families across ten representative leading editors, not a popularity ranking and not a claim that every editor has every feature. Example products are positive examples only; an omitted product is not a statement of absence. Features may depend on edition, operating system, camera, hardware or model availability.

The requested outcome is an easy automatic editor with professional override controls. This document defines the implementation work; it does not claim the missing features have been added. Vendor sources support observable capabilities. The AURA techniques below are proposed engineering approaches, not claims about undisclosed vendor algorithms.

Affinity is represented by its current photo-editing/Pixel Studio product. ON1's page advertises an upcoming 2027 release: this list uses its established-feature sections, not the upcoming release's additions. ACDSee's page transitions to 2027; the examples here use established editing capabilities, not newly announced generative tools. No product was purchased or installed for this comparison.

## Evidence and status

- **Verified**: the stated behavior passed the preceding five-portrait native check; see each row's narrower limitation. This is not a general quality certification.
- **Present**: implementation/UI evidence was inspected, but this research did not independently exercise the complete feature.
- **Partial**: a foundation exists with a known capability, integration, coverage or model-readiness gap.
- **Planned**: missing or not established in the inspected paths. This is not exhaustive proof that no related code exists anywhere in the repository.

The existing Graphify graph was used for navigation and then checked against current files. It predates this baseline and is not used as proof of current runtime behavior. README and model-specific documentation explicitly distinguish implemented infrastructure from untrained models. The earlier five-JPEG test verified import, enhancement repeatability, protected manual values, Undo/Redo, before/after controls, original hashes and five PNG exports; it did not validate RAW coverage, face AI or all 100 features.

## Ten reference products

- **PS: [Adobe Photoshop](https://www.adobe.com/products/photoshop/features.html)** — Layer compositing, selection and retouching.
- **LR: [Adobe Lightroom Classic](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html)** — Photo library and reversible RAW workflow.
- **C1: [Capture One Pro](https://www.captureone.com/en/products/capture-one-pro)** — Tethering and selective color control.
- **AF: [Affinity (photo editing / Pixel Studio)](https://www.affinity.studio/photo-editing-software)** — Layers, live filters and image merging.
- **DX: [DxO PhotoLab](https://www.dxo.com/dxo-photolab/features/)** — RAW denoising and measured optical correction.
- **ON: [ON1 Photo RAW](https://www.on1.com/products/photo-raw/features/?v=1&v=13)** — Effects, masks and multi-image workflows.
- **LU: [Luminar Neo](https://skylum.com/luminar-for-intel)** — Portrait tools and automatic scene enhancement.
- **AC: [ACDSee Photo Studio Ultimate](https://www.acdsee.com/en/products/photo-studio-ultimate/features/)** — Asset management and layered editing.
- **GI: [GIMP](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html)** — Pixel editing, paths and extensibility.
- **DT: [darktable](https://www.darktable.org/about/features/)** — Scene-referred RAW workflow and performance.

## Implementation sequence

1. **Professional daily editing.** Finish clipping overlays (19), selective synchronization (82), snapshot/version UX (85), white-balance sampling (12), complete brush/gradient workflows (41–43), and export watermarking (94). Reuse current recipe and history contracts where possible. Acceptance: edit, restart, Undo/Redo, compare and export each operation on real photos; original hashes stay unchanged.
2. **Color and RAW correctness.** Expand camera fixtures (2, 28, 35–36), input/display ICC handling (29), LUT import (30) and soft proofing (95). Acceptance: measured color targets, camera-specific decode fixtures, wide-gamut gradients, ICC reference transforms and documented unsupported cases.
3. **Useful local and portrait intelligence.** Validate models before enabling face/subject/sky masks (8, 46–49), then retouch (51, 54–60), object removal (53) and denoise (75). Model loading stays behind `InferService`. Acceptance: a consented, diverse, held-out image set; hair/veil boundaries, occlusion, multiple faces, dark skin, backlight and low light; explicit no-result behavior. Automatic edits must preserve manual decisions.
4. **Multiple-image processing.** Add alignment infrastructure (69), HDR (71), panoramas (72), focus stacks (73) and burst denoise (74). Acceptance: moving-subject/ghosting cases, parallax, focus breathing and memory-bounded full-resolution export. Never replace source images.
5. **Layered creative editing.** Design a versioned document model before adding layers (61–70), mesh warps (40), sky replacement (78), or generative fill/expand (79–80). Preserve existing recipe compatibility; use an ADR and explicit migration for frozen-contract changes. Acceptance: save/reopen exact compositions, nested masks, transparency, font handling and clearly scoped interchange.
6. **Performance and delivery.** Benchmark and activate GPU execution (98), tune caching (99), complete print/HDR output (96, 100), then add tethering (9) and extension workflows (87). Performance work starts with measurement during every earlier batch; it is not postponed until the end. Acceptance: p50/p95 edit-to-preview latency, peak memory, cancellation, device-loss recovery and CPU/GPU tolerance on named hardware.

Start with batches 1–3 for AURA's automatic portrait workflow. A general Photoshop-style compositor is a separate architectural expansion, not a set of extra sliders. No dates or quality scores are promised without measured estimates.

## Techniques and tools to evaluate

- **Existing Rust renderer and recipe journal:** keep one authoritative render pipeline for previews and export. Use a proposal → validation → protected merge → render → quality review → history sequence for automation. Check image orientation, alpha, color space and source provenance at boundaries.
- **Color transforms:** evaluate [Little CMS](https://www.littlecms.com/LittleCMS2.18%20tutorial.pdf) for ICC transformations and proofing. Its availability does not imply it is already integrated; any native dependency must satisfy AURA's current architecture rules.
- **GPU compute:** evaluate [wgpu](https://wgpu.rs/) behind the existing renderer abstraction. Retain the CPU reference, tile overlap, bounded resource use and documented numerical tolerance. GPU speed is a benchmark result, not a feature label.
- **Learned models:** evaluate [ONNX Runtime execution providers](https://onnxruntime.ai/docs/execution-providers/) behind `InferService`, with pinned model hashes, actual model cards and an offline fallback. On Windows, evaluate the [current WinML deployment guidance](https://onnxruntime.ai/docs/get-started/with-windows.html); DirectML is maintained but new Windows deployment development has moved toward WinML. A runtime alone does not supply trained models or rights to distribute their weights.
- **Local editing:** use normalized image coordinates, pressure-aware stroke samples, alpha masks, distance-field feathering and compositing in an explicit color space. Test borders at multiple zoom levels and output sizes.
- **Image merging:** registration precedes blending. Use robust transform estimation, motion masks, exposure normalization, multiscale fusion and intermediate provenance. Let users inspect ghosts and seams.
- **Retouching:** separate tone from texture, preserve identity detail, bound strengths and expose editable masks. A model must improve held-out photographs before it replaces the existing conservative correction.
- **Generative operations:** keep explicit user initiation, the original image, a mask, candidate provenance and cancellation. If a cloud provider is used, route it through the existing gateway and its established consent/settings flow. Do not make ordinary Auto Enhance upload images or invent content.

No new runtime dependency or model was installed for this roadmap. These are candidate approaches, not a claim to reproduce proprietary vendor internals.

## Completion gate for every feature

An implementation is complete only when its control changes the actual render, saves and reloads, participates correctly in history, handles cancellation/errors, respects manual edits, and agrees between preview and full-resolution export. Add meaningful operation-specific fixtures and a real-image acceptance case. The existing five portraits remain a smoke test; add RAW, high ISO, transparency, wide gamut, backlight, hair/veils, groups and large files before claiming broad coverage.

## The 100 features

Each numbered entry records its source examples, AURA evidence, current limitation, and proposed technique. Source paths are relative to this document's repository.

### Library and capture

1. **File and folder import** — Verified. Examples: LR, DT. [darktable features](https://www.darktable.org/about/features/).

    AURA: Five JPEG file roots passed native testing; other formats need their own fixtures. [Evidence](../crates/aura-ingest/src/import.rs). Proposed technique: Journal each import; hash originals; resume interrupted jobs.

2. **RAW decoding and demosaicing** — Partial. Examples: C1, DX. [Capture One Pro](https://www.captureone.com/en/products/capture-one-pro); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Coverage varies by compression; no claim of every camera. [Evidence](../docs/camera-support.md). Proposed technique: Extend existing safe decoder with camera-specific fixtures and fallback provenance.

3. **EXIF and camera/lens metadata** — Present. Examples: DT, DX. [darktable features](https://www.darktable.org/about/features/); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Inspect real camera MakerNotes before expanding support. [Evidence](../crates/aura-ingest/src/import.rs). Proposed technique: Normalize metadata once; preserve unknown fields and orientation.

4. **Ratings, flags and color labels** — Partial. Examples: DT, AC. [darktable features](https://www.darktable.org/about/features/); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Culling infrastructure is not proof of complete manual labeling UX. [Evidence](../crates/aura-catalog/migrations/0001_init.sql). Proposed technique: Persist user decisions separately from automatic culling suggestions.

5. **Keywords and searchable metadata** — Partial. Examples: DT, AC. [darktable features](https://www.darktable.org/about/features/); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Complete keyword editing and search UI not established. [Evidence](../crates/aura-catalog/migrations/0001_init.sql). Proposed technique: Index editable IPTC fields; add full-text search and bulk metadata updates.

6. **Collections and rule-based albums** — Partial. Examples: ON, AC. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Collections exist; general smart-album parity is unverified. [Evidence](../crates/aura-catalog/migrations/0001_init.sql). Proposed technique: Store collection membership and saved predicates independently.

7. **Duplicate detection and skip-on-import** — Present. Examples: LR. [Lightroom Classic import options](https://helpx.adobe.com/lightroom-classic/desktop/import-photos/photo-video-import-options.html).

    AURA: Exact reimport tests passed; near-duplicate visual matching is a separate extension. [Evidence](../crates/aura-ingest/src/import.rs). Proposed technique: Use content hashes and idempotent catalog writes; report skipped copies.

8. **Face recognition and people albums** — Partial. Examples: AC. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Face detector is documented as untrained. [Evidence](../docs/retouch.md). Proposed technique: Use a validated detector, embeddings and user-confirmed identity clusters.

9. **Tethered capture and live view** — Planned. Examples: C1, DT. [Capture One Pro](https://www.captureone.com/en/products/capture-one-pro); [darktable features](https://www.darktable.org/about/features/).

    AURA: No tethering path established in the inspected implementation. [Evidence](../docs/lightroom-parity.md). Proposed technique: Add camera transport adapters, capture queue and reconnect handling.

10. **Offline editable proxy previews** — Partial. Examples: LR. [Lightroom Classic Smart Previews](https://helpx.adobe.com/lightroom-classic/desktop/viewing-photos/lightroom-smart-previews.html).

    AURA: Cached previews alone do not prove offline editing. [Evidence](../crates/aura-preview/src/lib.rs). Proposed technique: Persist proxy provenance and reconcile edits when originals return.

### Exposure and tone

11. **Exposure compensation** — Verified. Examples: LR. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html).

    AURA: Manual exposure protection passed the five-photo native test. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Apply gain in linear light and journal the user's value.

12. **White balance temperature and tint** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Controls exist; camera-specific accuracy still needs measured targets. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Use chromatic adaptation with an optional neutral-patch picker.

13. **Global contrast** — Present. Examples: LR, ON. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Control and renderer exist; no competitor-quality benchmark. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Use bounded tone shaping with stable neutral settings.

14. **Highlight recovery** — Partial. Examples: LR, DX. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: JPEG highlight adjustment cannot recover clipped sensor data. [Evidence](../crates/aura-render/src/cpu.rs). Proposed technique: Recover RAW channel detail before tone mapping; protect specular highlights.

15. **Shadow recovery** — Present. Examples: LR. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html).

    AURA: Needs high-ISO and backlit RAW validation. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Lift dark tonal ranges with noise-aware limits.

16. **White and black point controls** — Present. Examples: LR. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html).

    AURA: Dedicated sliders are present. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Adjust endpoints with smooth shoulders and clipping feedback.

17. **Levels and midpoint gamma** — Planned. Examples: AC, GI. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/); [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Separate Levels tool not established. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Add input/output black-white points and midpoint gamma.

18. **RGB and luminance histogram** — Verified. Examples: LR, DX. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Current histogram is a preview sample, not RAW sensor data. [Evidence](../ui/src/components/develop/Histogram.tsx). Proposed technique: Measure rendered pixels with bounded sampling and channel bins.

19. **Highlight and shadow clipping overlay** — Partial. Examples: LR. [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html).

    AURA: Near-clipping percentages exist; spatial overlay is missing. [Evidence](../ui/src/components/develop/Histogram.tsx). Proposed technique: Overlay per-pixel clipping masks with independent channel thresholds.

20. **One-click automatic tone enhancement** — Verified. Examples: LU, LR. [Luminar Neo tools](https://skylum.com/luminar-for-intel); [Lightroom Classic tone and clipping](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/image-tone-color.html).

    AURA: Local measured correction, not a trained subject-aware model. [Evidence](../crates/aura-app/src/photo_enhance.rs). Proposed technique: Use bounded luminance statistics; protect manual fields; keep a neutral option.

### Color and creative control

21. **Point and individual RGB tone curves** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Controls and renderer exist. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Use monotonic interpolation and cache transfer tables.

22. **Parametric tone curves** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Controls and renderer exist. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Blend smooth tonal bands with ordered split points.

23. **HSL color mixer** — Present. Examples: LR, C1. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [Capture One Color Editor](https://support.captureone.com/hc/en-us/articles/360002601358-The-Color-Editor-overview).

    AURA: Skin protection needs varied real-portrait evaluation. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Apply soft hue-band weights with stable hue wrapping.

24. **Targeted point-color editing** — Planned. Examples: C1. [Capture One Color Editor](https://support.captureone.com/hc/en-us/articles/360002601358-The-Color-Editor-overview).

    AURA: Fixed HSL bands are not a sampled point-color tool. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Sample a pixel; edit a feathered range in perceptual color space.

25. **Shadow/midtone/highlight color grading** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: UI and recipe fields are present. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Blend tonal color wheels while controlling luminance changes.

26. **Black-and-white channel mixing** — Present. Examples: ON. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Monochrome controls exist. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Mix source hue contributions into monochrome luminance.

27. **Vibrance and saturation** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Controls and renderer exist. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Separate global chroma scaling from bounded low-chroma enhancement.

28. **Camera calibration and matching profiles** — Partial. Examples: LR, C1. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [Capture One Pro](https://www.captureone.com/en/products/capture-one-pro).

    AURA: Calibration controls exist; real camera profiles are missing. [Evidence](../docs/colour-management.md). Proposed technique: Fit profiles from measured targets under controlled illuminants.

29. **ICC input/display/output color management** — Partial. Examples: DT, LR. [darktable features](https://www.darktable.org/about/features/); [Lightroom Classic color management](https://helpx.adobe.com/lightroom-classic/desktop/workspace/color-management.html).

    AURA: Output spaces exist; arbitrary input ICC and display parity are incomplete. [Evidence](../docs/colour-management.md). Proposed technique: Keep scene-linear working pixels; transform through explicit ICC profiles.

30. **Importable 3D LUT looks** — Planned. Examples: ON, AC. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Internal curve LUTs are not user-importable 3D LUTs. [Evidence](../crates/aura-render/src/creative.rs). Proposed technique: Parse cube/3DL safely; interpolate a 3D lattice with amount control.

### Geometry and optics

31. **Crop and aspect-ratio presets** — Present. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Existing presets and crop recipe need broader native coverage. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Store normalized crop coordinates with reversible aspect locks.

32. **Straighten and horizon correction** — Present. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Manual control exists; automatic quality requires validation. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Use reversible rotation and optional measured line suggestions.

33. **Rotate and mirror** — Partial. Examples: GI. [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Rotation exists; a complete mirror workflow is unverified. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Compose orientation transforms and update crop/mask coordinates.

34. **Perspective and keystone correction** — Present. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Code exists; architectural real-image cases remain to be tested. [Evidence](../crates/aura-render/src/cpu.rs). Proposed technique: Apply a homography with bounded interpolation and crop preview.

35. **Lens distortion correction** — Partial. Examples: DX, LR. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Measured camera/lens coverage is incomplete. [Evidence](../docs/lightroom-parity.md). Proposed technique: Apply calibrated radial/tangential lens models.

36. **Chromatic aberration and defringing** — Partial. Examples: DX, LR. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Correction controls exist; universal lens parity is unproven. [Evidence](../docs/lightroom-parity.md). Proposed technique: Align color channels and suppress selected edge fringes.

37. **Lens vignetting compensation** — Present. Examples: DX, LR. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Calibration quality remains camera/lens dependent. [Evidence](../crates/aura-render/src/cpu.rs). Proposed technique: Apply calibrated radial gain before creative grading.

38. **High-quality image resampling** — Present. Examples: GI, ON. [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Code exists; compare downsampling and enlargement fixtures. [Evidence](../docs/delivery.md). Proposed technique: Resize in linear light with antialiasing and tile borders.

39. **Content-aware scaling** — Planned. Examples: PS. [Photoshop content-aware scaling](https://helpx.adobe.com/ca/photoshop/desktop/crop-resize-transform/resize-adjust-resolution/preserve-visual-content-when-scaling-images.html).

    AURA: No implementation established; keep geometry changes reversible. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Evaluate seam carving with user-protected subject regions.

40. **Liquify and mesh warping** — Planned. Examples: AF, ON. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Manual creative tool only; do not apply automatically to faces. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Use editable displacement meshes with foldover limits.

### Selections and local adjustments

41. **Local adjustment brush** — Partial. Examples: LR, DX. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Mask-editing infrastructure exists; complete painting UX is unverified. [Evidence](../ui/src/components/develop/MaskPanel.tsx). Proposed technique: Rasterize pressure-aware strokes into tiled alpha masks.

42. **Linear gradient masks** — Partial. Examples: LR, DX. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Renderer support exists; direct on-image workflow needs validation. [Evidence](../docs/lightroom-parity.md). Proposed technique: Evaluate a feathered signed-distance ramp in image coordinates.

43. **Radial gradient masks** — Partial. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Renderer support exists; direct manipulation needs validation. [Evidence](../docs/lightroom-parity.md). Proposed technique: Evaluate elliptical distance with feather, invert and rotate.

44. **Luminance-range masks** — Planned. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: General user-authored range-mask tool not established. [Evidence](../ui/src/components/develop/MaskPanel.tsx). Proposed technique: Map luminance into a soft threshold band.

45. **Color-range masks** — Partial. Examples: LR, DX. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Color growth is present; full editable range workflow unverified. [Evidence](../docs/masks.md). Proposed technique: Use perceptual color distance with tolerance and feather.

46. **Automatic subject/background selection** — Partial. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Segmentation heads are untrained. [Evidence](../docs/lightroom-parity.md). Proposed technique: Run validated segmentation through the existing inference service.

47. **Automatic sky selection** — Partial. Examples: LR, DX. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Real-photo automatic masks are blocked by model readiness. [Evidence](../docs/lightroom-parity.md). Proposed technique: Segment sky and refine edges against foreground structures.

48. **Face, skin, hair and eye masks** — Partial. Examples: LR, LU. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [Luminar portrait tools](https://support.skylum.com/editing-tools/portrait-tools).

    AURA: Untrained face detection blocks real portrait masks. [Evidence](../docs/retouch.md). Proposed technique: Combine face landmarks with semantic parsing and matting.

49. **Mask feathering and edge refinement** — Partial. Examples: DX, AC. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Basic operations exist; hair/veil quality is unproven. [Evidence](../ui/src/components/develop/MaskPanel.tsx). Proposed technique: Use distance fields and edge-aware matting; expose mask preview.

50. **Mask add, subtract, intersect and invert** — Partial. Examples: LR, DX. [Lightroom Classic mask combinations](https://helpx.adobe.com/dk/lightroom-classic/desktop/process-and-develop-photos/masking.html); [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/).

    AURA: Complete boolean authoring workflow needs verification. [Evidence](../ui/src/components/develop/MaskPanel.tsx). Proposed technique: Represent combinations as a reversible alpha-expression graph.

### Portraits and repair

51. **Spot healing and blemish repair** — Partial. Examples: PS, AC. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Automatic face/blemish models are not ready for real portraits. [Evidence](../docs/retouch.md). Proposed technique: Provide explicit source sampling and edge-aware patch blending.

52. **Clone stamp** — Planned. Examples: PS, ON. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: General manual clone-stamp workflow not established. [Evidence](../docs/retouch.md). Proposed technique: Paint from a chosen source offset with opacity and edge feather.

53. **Content-aware object removal** — Partial. Examples: PS. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html).

    AURA: Current automatic cleanup refuses unclassified real-image removals. [Evidence](../docs/lightroom-parity.md). Proposed technique: Start with user-selected masks; evaluate patch search and inpainting.

54. **Red-eye correction** — Partial. Examples: LR, AC. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Automatic path depends on unavailable face evidence. [Evidence](../docs/retouch.md). Proposed technique: Detect pupil/redness within a user-confirmed eye region.

55. **Texture-preserving skin smoothing** — Partial. Examples: LU. [Luminar portrait tools](https://support.skylum.com/editing-tools/portrait-tools).

    AURA: Retouch safeguards exist; usable real-photo masks are missing. [Evidence](../docs/retouch.md). Proposed technique: Separate low-frequency variation from texture with strict strength limits.

56. **Frequency separation** — Planned. Examples: AF, AC. [Affinity frequency separation](https://www.affinity.studio/blog/frequency-separation-explained); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: A full editable frequency-separation workflow is not established. [Evidence](../docs/retouch.md). Proposed technique: Create reversible low/high-frequency components with neutral reconstruction.

57. **Dodge and burn / local relighting** — Partial. Examples: PS, LU. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [Luminar Neo tools](https://skylum.com/luminar-for-intel).

    AURA: Local-light infrastructure exists; depth-aware relighting is not proven. [Evidence](../docs/lightroom-parity.md). Proposed technique: Apply masked exposure and edge-aware luminance changes.

58. **Eye and teeth enhancement** — Partial. Examples: LU, AC. [Luminar portrait tools](https://support.skylum.com/editing-tools/portrait-tools); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Real-photo face detection remains a dependency. [Evidence](../docs/retouch.md). Proposed technique: Bound local luminance/chroma edits inside confirmed masks.

59. **Skin-tone uniformity** — Partial. Examples: C1. [Capture One Color Editor](https://support.captureone.com/hc/en-us/articles/360002601358-The-Color-Editor-overview).

    AURA: Measured retouch code exists; accuracy on people is unproven. [Evidence](../docs/retouch.md). Proposed technique: Reduce unwanted hue/chroma variance within protected skin masks.

60. **Depth-aware background blur** — Planned. Examples: LR, LU. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [Luminar portrait tools](https://support.skylum.com/editing-tools/portrait-tools).

    AURA: No complete depth-bokeh workflow established. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Estimate depth and alpha; use occlusion-aware variable-radius blur.

### Layers and composition

61. **Pixel layers and layer groups** — Planned. Examples: AF, GI. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Current edit recipe is not a general layered document. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Add a versioned document graph with ordered children and stable IDs.

62. **Adjustment layers** — Planned. Examples: AC. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Global recipe blocks are not user-managed adjustment layers. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Represent adjustments as independently masked graph nodes.

63. **Per-layer masks** — Planned. Examples: AC, GI. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/); [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Requires a layered document model. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Attach editable alpha planes to compositing nodes.

64. **Layer blend modes and opacity** — Planned. Examples: AC, GI. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/); [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Local edit strength is not general layer blending. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Implement premultiplied-alpha compositing with explicit blend space.

65. **Smart Objects / linked image objects** — Planned. Examples: PS. [Photoshop Smart Objects](https://helpx.adobe.com/photoshop/desktop/create-manage-layers/smart-objects/smart-objects-overview-and-benefits.html).

    AURA: Requires asset references, relinking and nested rendering. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Reference immutable source assets and retain transform/filter parameters.

66. **Reorderable nondestructive filter stack** — Partial. Examples: GI, AF. [GIMP nondestructive filters](https://docs.gimp.org/3.2/en_GB/gimp-filters-common.html); [Affinity photo editing](https://www.affinity.studio/photo-editing-software).

    AURA: Existing fixed renderer stages are not a reorderable user stack. [Evidence](../crates/aura-render/src/cpu.rs). Proposed technique: Store versioned operation nodes with explicit dependencies.

67. **Editable text layers** — Planned. Examples: PS, AC. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: No layered typography workflow established. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Add font shaping, text layout and resolution-independent rendering.

68. **Vector shapes and editable paths** — Planned. Examples: PS, GI. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [GIMP 3 documentation index](https://docs.gimp.org/3.0/en_GB/gimp-help-index.html).

    AURA: Needs document nodes and selection/path editing tools. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Store Bezier paths and rasterize with antialiasing at output scale.

69. **Automatic layer alignment and blending** — Planned. Examples: AC. [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: No multi-source composition graph established. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Estimate transforms, compensate exposure and blend overlapping regions.

70. **Editable layered document interchange** — Planned. Examples: AF, ON. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Current XMP recipes do not preserve arbitrary layered documents. [Evidence](../crates/aura-recipe/src/contract/recipe.rs). Proposed technique: Define native layered serialization; explicitly scope PSD/ORA interoperability.

### Computational photography

71. **Bracketed HDR merge** — Planned. Examples: AF, ON. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Explicitly listed as missing. [Evidence](../docs/lightroom-parity.md). Proposed technique: Align brackets, reject motion and merge scene radiance before tone mapping.

72. **Panorama stitching** — Planned. Examples: ON, AC. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [ACDSee Photo Studio Ultimate features](https://www.acdsee.com/en/products/photo-studio-ultimate/features/).

    AURA: Explicitly listed as missing. [Evidence](../docs/lightroom-parity.md). Proposed technique: Match features, solve camera geometry, choose seams and multiband blend.

73. **Focus stacking** — Planned. Examples: AF, ON. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: No implementation established. [Evidence](../docs/lightroom-parity.md). Proposed technique: Align scale/focus breathing; fuse sharp regions with seam refinement.

74. **Multi-frame noise reduction** — Planned. Examples: PS. [Photoshop image stacks and Smart Objects](https://helpx.adobe.com/photoshop/desktop/create-manage-layers/create-layer-compositions/create-and-process-image-stacks.html).

    AURA: Current restoration is not a proven multi-frame merge tool. [Evidence](../docs/restoration.md). Proposed technique: Align bursts and robustly aggregate static regions with motion rejection.

75. **AI denoising** — Partial. Examples: DX, ON. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Classical restoration exists; trained model parity is unproven. [Evidence](../docs/restoration.md). Proposed technique: Evaluate trained denoisers against classical baselines and identity detail.

76. **Sharpening, texture, clarity and dehaze** — Present. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Controls exist; halo/noise behavior needs broader image tests. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Use scale-aware local contrast, edge masks and bounded restoration.

77. **AI enlargement / super-resolution** — Planned. Examples: ON, PS. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [Photoshop tools](https://www.adobe.com/products/photoshop/features.html).

    AURA: Do not confuse standard resizing with learned recovery. [Evidence](../docs/restoration.md). Proposed technique: Tile a validated super-resolution model with overlap and detail safeguards.

78. **Sky replacement** — Planned. Examples: LU, ON. [Luminar Neo tools](https://skylum.com/luminar-for-intel); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Requires reliable masks and multiple source assets. [Evidence](../docs/lightroom-parity.md). Proposed technique: Combine sky matting, horizon alignment and editable scene relighting.

79. **Generative fill** — Planned. Examples: PS. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html).

    AURA: Cleanup infrastructure is not a general prompt-fill product. [Evidence](../docs/lightroom-parity.md). Proposed technique: Store a selected-mask request and versioned generated candidates.

80. **Generative canvas expansion** — Planned. Examples: PS, LU. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [Luminar Neo tools](https://skylum.com/luminar-for-intel).

    AURA: Requires explicit user initiation and provider/local-model integration. [Evidence](../docs/lightroom-parity.md). Proposed technique: Extend document bounds and generate only the user-selected missing region.

### Workflow and automation

81. **Presets and adjustable preset strength** — Present. Examples: PS, ON. [Photoshop tools](https://www.adobe.com/products/photoshop/features.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Existing presets; custom authoring/import/export still need review. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Save versioned parameter subsets and interpolate supported values.

82. **Copy, paste and synchronize edits** — Present. Examples: LR. [Lightroom Classic copy and synchronize](https://helpx.adobe.com/in/lightroom-classic/desktop/help/applying-adjustments-develop-module-basic.html).

    AURA: Current sync-all behavior needs selective sync expansion. [Evidence](../ui/src/components/develop/LightroomPanel.tsx). Proposed technique: Apply selected recipe fields in a transactional batch.

83. **Reference-image look matching** — Partial. Examples: C1. [Capture One new features](https://www.captureone.com/en/explore-features/whats-new).

    AURA: A statistical approximation, not recovery of another editor's settings. [Evidence](../ui/src/components/look/MatchLookPanel.tsx). Proposed technique: Match bounded color/tone statistics and validate through the renderer.

84. **Nondestructive history and Undo/Redo** — Verified. Examples: DX, DT. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [darktable features](https://www.darktable.org/about/features/).

    AURA: Five-photo tests passed Undo/Redo and new-edit branch behavior. [Evidence](../crates/aura-app/src/develop_commands.rs). Proposed technique: Replay an append-only edit journal with explicit cursor movement.

85. **Virtual copies and named snapshots** — Planned. Examples: DX, LR. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html).

    AURA: Persistent history is not a complete virtual-copy/snapshot UI. [Evidence](../crates/aura-app/src/develop_commands.rs). Proposed technique: Fork recipe references without duplicating originals.

86. **Before/after comparison and zoom** — Verified. Examples: LR, GI. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [GIMP nondestructive filters](https://docs.gimp.org/3.2/en_GB/gimp-filters-common.html).

    AURA: Native comparison passed; exhaustive synchronized zoom parity untested. [Evidence](../ui/src/components/develop/PhotoStudio.tsx). Proposed technique: Render matched views with a keyboard-accessible divider.

87. **Macros, actions and plug-in scripting** — Planned. Examples: AF, GI. [Affinity photo editing](https://www.affinity.studio/photo-editing-software); [GIMP plug-ins and scripting](https://developer.gimp.org/resource/about-plugins/).

    AURA: Autopilot stages are not arbitrary user-authored macros. [Evidence](../ui/src/components/autopilot/prepareCollection.ts). Proposed technique: Record typed commands; validate parameters; expose a bounded extension API.

88. **Batch editing and export queues** — Present. Examples: ON, AF. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [Affinity photo editing](https://www.affinity.studio/photo-editing-software).

    AURA: Five-image export passed; large-gallery capacity needs measurement. [Evidence](../ui/src/components/autopilot/prepareCollection.ts). Proposed technique: Use bounded workers, cancellation checkpoints and per-item results.

89. **Film grain and analog looks** — Present. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Creative controls exist; visual calibration is not benchmarked. [Evidence](../crates/aura-render/src/creative.rs). Proposed technique: Use seeded spatial grain so preview and export agree.

90. **Creative post-crop vignette** — Present. Examples: LR, ON. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Controls and renderer exist. [Evidence](../crates/aura-render/src/creative.rs). Proposed technique: Evaluate feathered geometry in final output coordinates.

### Output and performance

91. **JPEG/PNG/TIFF export and bit-depth options** — Partial. Examples: DT, LR. [darktable features](https://www.darktable.org/about/features/); [Lightroom Classic export](https://helpx.adobe.com/lightroom-classic/desktop/export-photos/export-files-disk-or-cd.html).

    AURA: Five PNG exports verified; format/depth matrix is not fully tested. [Evidence](../crates/aura-export/src/api.rs). Proposed technique: Keep format capabilities explicit; verify decoded output after writing.

92. **Export resizing and output sharpening** — Present. Examples: ON, LR. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13); [Lightroom Classic export](https://helpx.adobe.com/lightroom-classic/desktop/export-photos/export-files-disk-or-cd.html).

    AURA: Do not promise identical print appearance without profiling. [Evidence](../docs/delivery.md). Proposed technique: Resize once, then sharpen for the chosen output size and medium.

93. **Batch naming and metadata/privacy controls** — Present. Examples: DX, LR. [DxO PhotoLab features](https://www.dxo.com/dxo-photolab/features/); [Lightroom Classic export](https://helpx.adobe.com/lightroom-classic/desktop/export-photos/export-files-disk-or-cd.html).

    AURA: Export policy exists; broaden metadata round-trip tests. [Evidence](../docs/delivery.md). Proposed technique: Plan collision-safe names; whitelist metadata; strip location when chosen.

94. **Text and image watermarks** — Planned. Examples: LR, ON. [Lightroom Classic export](https://helpx.adobe.com/lightroom-classic/desktop/export-photos/export-files-disk-or-cd.html); [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: A full watermark editor/export path is not established. [Evidence](../docs/delivery.md). Proposed technique: Composite a positioned overlay after resize in output space.

95. **Soft proofing and gamut warnings** — Planned. Examples: LR. [Lightroom Classic color management](https://helpx.adobe.com/lightroom-classic/desktop/workspace/color-management.html).

    AURA: Explicitly listed as missing. [Evidence](../docs/lightroom-parity.md). Proposed technique: Use display and printer profiles with selectable rendering intent.

96. **Print layouts and contact sheets** — Planned. Examples: ON. [ON1 Photo RAW established features](https://www.on1.com/products/photo-raw/features/?v=1&v=13).

    AURA: Exporting an album set is not a desktop print-layout engine. [Evidence](../docs/delivery.md). Proposed technique: Lay out physical page units, margins and color-managed output.

97. **XMP sidecars and edit interchange** — Partial. Examples: DT, LR. [darktable features](https://www.darktable.org/about/features/); [Lightroom Classic copy and synchronize](https://helpx.adobe.com/in/lightroom-classic/desktop/help/applying-adjustments-develop-module-basic.html).

    AURA: Writer exists; full cross-editor round-trip fidelity is not proven. [Evidence](../crates/aura-recipe/src/xmp.rs). Proposed technique: Map supported parameters; retain unmapped data; document lossy translation.

98. **GPU-accelerated rendering** — Partial. Examples: DT, AF. [darktable features](https://www.darktable.org/about/features/); [Affinity photo editing](https://www.affinity.studio/photo-editing-software).

    AURA: Shaders exist but no GPU backend is linked. [Evidence](../crates/aura-render/src/lib.rs). Proposed technique: Connect a compute backend behind the renderer; keep tested CPU fallback.

99. **Tiled rendering and multiresolution caching** — Present. Examples: DT. [darktable features](https://www.darktable.org/about/features/).

    AURA: Architecture exists; interactive latency targets need real measurement. [Evidence](../crates/aura-preview/src/lib.rs). Proposed technique: Cache by source/recipe/version; bound memory; prioritize visible tiles.

100. **HDR display editing and HDR export** — Planned. Examples: LR. [Lightroom Classic Develop tools](https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/develop-module-tools.html); [Lightroom Classic export](https://helpx.adobe.com/lightroom-classic/desktop/export-photos/export-files-disk-or-cd.html).

    AURA: Wide-gamut floating-point internals alone are not HDR delivery. [Evidence](../docs/colour-management.md). Proposed technique: Add an HDR display/output path with SDR fallback and gain-map validation.

## Research outputs

The companion `photo-editor-feature-roadmap.json` contains the same 100 records for filtering and future tracking. Status counts describe this scoped inspection, not a product-completeness percentage. This pass changes documentation only; none of the planned capabilities is marked shipped.
