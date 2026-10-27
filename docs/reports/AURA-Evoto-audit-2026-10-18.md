# AURA and Evoto: implementation audit and development report

Date: 18 October 2026. Target: the AURA source in this workspace and Evoto installed at `F:\evoto`. Requested finish: polished wedding portraits, with strong temporary-blemish and under-eye cleanup, retained facial detail, protected ceremonial marks, and no artificial skin rims. Dodge and burn and frequency separation remain restricted to suitable single-person portraits under the user's chosen automatic policy.

## 1. What this audit establishes

**AURA has a substantial native editing engine, but Evoto-equivalent portrait quality has not been demonstrated. More sliders, stronger smoothing, and additional geometric exclusions will not by themselves close the gap.** The main priorities are finer anatomical segmentation, a trained temporary-defect detector, better uncertainty handling, consistent mask ownership across workflows, regional pixel verification, and a representative quality benchmark.

The user's examples expose related but distinct failures: an artificial band at the chin/neck, damaged nose-wing contours, a separate-looking under-eye colour patch, untreated skin beside the tika and hairline, and flattened or displaced facial transitions. Calling these all “missed blemishes” leads to the wrong fix. Some are **over-correction of real anatomy**, some are **selection errors**, and others are **a mismatch between a correction's spatial scale and the feature it crosses**. The report therefore separates detection, selection, correction and verification.

This is an evidence-based partial reverse-engineering and source audit, not a recovery of Evoto's complete private implementation. No Evoto model architecture, weight set, training corpus, loss function, exact white-balance solver or complete processing graph has been recovered. Nothing in this report establishes that Evoto selects skin perfectly on every image. Its public documentation itself provides manual refinement tools. An exact pixel replica cannot honestly be promised from module names, binary strings, public controls or a few examples.

Evidence categories used below:

| Label | Meaning |
|---|---|
| Local observation | Read directly from the installed files or running UI; scope stated explicitly. |
| AURA source finding | Verified in the current implementation, not inferred from a feature name. |
| Public behaviour | Documented by Evoto; this describes controls/results, not private internals. |
| Independent proposal | A design AURA can implement and validate itself; not attributed to Evoto. |
| Unknown | Evidence is insufficient. |

## 2. REA installation and inspection status

The requested [morluto/rea project](https://github.com/morluto/rea) was reviewed and cloned into `.work-checks/evoto-audit-2026-10-18/rea-source`. The reviewed source revision is `8f7d12e30d63f3d3b4cfe2a90336362841438f56`. The checkout declares 6.6.0, while the npm registry supplied 6.5.0. Attempting to install 6.6.0 returned `ETARGET`; published **6.5.0 was installed and verified with `rea.cmd --version`**.

Installation used `npm.cmd install --global rea-agents@6.5.0 --ignore-scripts` on Node 22.23.2. The `.cmd` launcher avoids the host's PowerShell script execution restriction without changing that policy. Package lifecycle scripts were skipped. No agent configuration or other integrations were silently rewritten. The CLI works independently of an MCP registration. `rea doctor` initially reported missing decompiler engines and agent registrations; that does not invalidate the successful objdump commands.

REA was actually used, rather than merely installed:

| Command | Verified result |
|---|---|
| `inspect-with-objdump F:\evoto\Evoto.exe file_headers` | x86-64 Windows PE executable. Source SHA-256 unchanged after inspection. |
| `inspect-with-objdump F:\evoto\Evoto-worker.exe symbols` | GNU objdump reports `no symbols` in the COFF symbol table. This does not mean all strings, imports, unwind data or recoverable functions are absent. |
| `inspect-with-objdump F:\evoto\Evoto-worker.exe section_headers` | Ten PE sections, including `.text`, `.rdata`, `.pdata`, `.cb00`, `.cb01`, `.cb02`. Section names alone do not identify a protection product or establish that all code is encrypted. |

Raw evidence envelopes, tool parameters, source hashes, stdout/stderr and limitations are saved in `.work-checks/evoto-audit-2026-10-18/*.evidence.json`. Direct GNU objdump PE-import observations are saved separately as `worker-pe.txt`; they are not mislabeled as REA-reconstructed source.

For deeper static inspection, portable Ghidra 12.1.4 and Temurin JDK 21.0.12.1+1 were downloaded from their publishers' GitHub releases, SHA-256 checked, and extracted under the audit directory. This leaves system Java, registry and application files unchanged. Ghidra release digest: `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`; JDK digest: `f9d6e191ab098c0d416e7d588a24420a8621cd2f4720dab2459b8b7b2d2d8b4e`. The deep-inspection result and its actual limitations are recorded in the validation addendum below. Installation does not itself prove successful decompilation. See [REA installation](https://github.com/morluto/rea/blob/main/docs/installation.md), [Ghidra releases](https://github.com/NationalSecurityAgency/ghidra/releases/tag/Ghidra_12.1.4_build) and [Temurin release](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.12.1%2B1).

## 3. What the installed Evoto files reveal

The installed main executable reports **7.3.5.0**. Current online documentation may describe a newer version or a different platform; those features must not automatically be counted as verified capabilities of local 7.3.5.

| Artifact | Size | SHA-256 |
|---|---:|---|
| `Evoto.exe` | 297,899,920 bytes | `469a173c19567f0a90222d192ad15694a2929005402ed731a9f51f42b83e302d` |
| `Evoto-worker.exe` | 282,190,224 bytes | `99edcbb0df9b063ca6fae3bb9603b15dcc41994532cc3cde7434e66404078f86` |

The installation contains Qt 5 GUI libraries, image/metadata and multimedia libraries, OpenGL-related libraries and several camera SDKs. The worker's PE import table directly identifies Qt and Windows runtime dependencies. These observations support a native Qt application architecture; they do not reveal the retouching sequence or prove that a particular camera SDK is used for every import.

Both main and worker binaries contain 1,049 literal occurrences of `onnxruntime`. That is evidence that ONNX Runtime-related code/text is present, and supports investigating ONNX inference integration. It does **not** identify the network used for a blemish or facial mask. A short token such as `MNN` can also occur incidentally; token counts are not sufficient to identify a second inference engine. Absence of a standalone inference DLL also does not rule out static linking or dynamically loaded providers.

There are **100 named effect module directories** and **97 `effect.json` carriers** under `resources/effect/config`. All 97 examined carriers fail plain UTF-8 JSON parsing. Their common opaque representation has not been decoded. A `.json` extension therefore does not give us the actual algorithm parameters. `resources/pixcook` is a 1,436,078,092-byte opaque resource package; its contents and precise encoding have not been established. Entropy or unusual headers alone do not establish encryption.

Selected module families are useful investigative leads:

| Family | Observed resource names | What can be concluded |
|---|---|---|
| Face/body defects | `FaceAcne`, `FaceAcneBaby`, `BodyAcne`, `Wrinkle`, `NeckLines`, `NoseFlaw` | The shipped resource organization distinguishes these tasks. The lesion classifiers and exact thresholds remain unknown. |
| Skin bands and texture | `FaceDermabrasionHLF`, `BodyDermabrasionHLF`, `FacePortraiture`, `BodyPortraiture`, `SkinTexture` | Separate face/body and band/texture-related resources exist. No frequency kernel, decomposition radius or learned model is established. |
| Protection and boundaries | `BeardProtect`, `MakeupProtect`, `MatteRefinement`, `MatteRefinementMix` | Protection/refinement are explicit resource concepts. Their input labels, ordering and thresholds remain unknown. |
| Under-eyes and eye detail | `RemoveEyeBag`, `RemoveDoubleEyelid`, `BrightEye`, `EyeWhiten`, `EyeRemoveRedBlood`, `IrisEditing` | Eye subproblems are distinguished; an exact eye-protection mask cannot be inferred from these names. |
| Skin colour/light | `AISkinUnify`, `MultiplePeopleAISkinUnify`, `BodySkinUnify`, `NeutralGrayDB`, `AIShine`, `Degreasing` | Separate task resources exist. They do not prove a particular neural architecture or illuminant estimator. |
| Additional workflow | `LensCorrect`, `PerspectiveTransform`, `ClothesCrease`, `BgClean`, `GlassReflectionRemoval`, `RealGrain` | Useful capability leads for a versioned product inventory, not copied implementations. |

The running UI was inspected through Windows UI Automation without invoking export. It exposed Library/Edit navigation, project and folder navigation, Smart Culling and the selected `989A2698.CR3` with camera metadata. An attempted Edit-view switch did not establish that the complete edit panel was available, so no hidden controls or slider response curves are claimed as observed. No paid export, licence bypass, account credential inspection or model-resource copying was performed.

Reproduce the bounded installation audit with:

```powershell
python scripts/audit-evoto-install.py --root 'F:\evoto' --output .work-checks/evoto-audit-2026-10-18/install-inventory.json
```

This script reads explicitly named installation binaries and effect configurations. It does not scan user profiles, projects, passwords or network traffic. Its JSON includes hashes, PE section metadata, literal counts, config sizes and parsing status. None of those measurements is labeled as recovered source code.

## 4. Observable Evoto behaviour worth matching

Evoto publicly documents separate blemish and under-eye controls, lower-eyelid protection, skin smoothing approaches, regional skin-colour work and manual mask correction. These are appropriate behavioural targets. The documented lower-eyelid protection is particularly relevant: reducing a dark circle and flattening the lower eyelid are different operations. AURA should preserve that distinction. [Evoto blemish controls](https://support.evoto.ai/blemish-removal/)

Its skin-retouching guide describes dodge-and-burn, textured smoothing and high/low-frequency controls, as well as face/body skin work and manual tuning. This documents user-facing methods, but does not disclose the exact filters, masks, training or execution order. [Evoto skin-retouching guide](https://support.evoto.ai/skin-retouching/)

AI Color Match publicly describes reference-based appearance matching with semantic awareness and controls for overall amount, tone and colour. This is a broader target than matching a finished export of the exact same photograph through nine global sliders. It is also distinct from technical white-balance correction. [Evoto AI Color Match](https://support.evoto.ai/feature-introduction-ai-color-match/)

The colour module documents RAW-aware lens correction, colour controls and reference/style workflows; some sections explicitly describe mobile behaviour. AURA should implement and validate desktop requirements separately rather than copy a mixed-platform feature list uncritically. [Evoto colour module](https://support.evoto.ai/color-adjustments-module/)

Evoto also documents optional generative repair and background tools, often with manual refinement and per-operation credits. Those are separate from faithful non-generative portrait cleanup. AURA should not claim equivalence from a generic image-generation endpoint or let it silently replace facial details. [Evoto AI Lab](https://support.evoto.ai/ai-lab/)

## 5. What AURA already does well

AURA can ingest camera files and render through its own native RAW pipeline. Current source includes camera-native decoders, demosaicing, source black/white-level handling, camera colour matrices, a linear Rec.2020 working space, editable recipe operations, sRGB/Adobe RGB/P3 output paths, and 16-bit delivery support. Recent Canon CR3 full-resolution review/export work is real evidence for those tested files. It does not certify every CR2/CR3/ARW variant, camera body, compression mode or lens.

The main automatic portrait path uses a real bundled YuNet detector (`crates/aura-vision/src/portrait.rs`), including multi-scale work, and a real MediaPipe Selfie Multiclass segmenter (`crates/aura-vision/src/skin.rs`). It is incorrect to describe all AURA AI as placeholder inference. The model assets and hashes are explicit. The skin segmenter labels background, hair, body skin, face skin, clothing and other; person crops improve small-subject handling.

Native skin operations already use selected-pixel neighbourhood means, same-person colour references, guided filtering and bounded corrections. Current masks preserve authored holes and can contract contours without expanding outside their support. Recent source guards protect under-eye/socket structure, nose openings, jaw/neck boundaries, expression contours and ceremonial marks. AURA also has actual healing/donor search, ring fitting, frequency healing, texture handling and editable operation provenance. These are substantial foundations worth retaining.

The application has culling, colour/style, masking, portrait, QC and export service layers. Existing export verification checks material properties such as successful writing/decoding and sealed delivery records. The separate `aura-qc` service has a sound contract for distinguishing unavailable readings from clean readings. The problem is coverage and integration across all current workflows, not a complete absence of QC architecture.

## 6. Most important gaps, with code evidence

| Priority | Gap | Current evidence | Required improvement |
|---|---|---|---|
| P0 | Fine anatomical masks | The automatic detector provides five landmarks. Selfie segmentation is six-class, 256-input inference with a 1024 working edge and stored mattes up to 512 cells. | Dense, confidence-calibrated facial geometry plus fine semantic face parsing and native-resolution boundary refinement. |
| P0 | Temporary-defect semantics | `aura-retouch/src/ops.rs` has `BLEMISH_HEAD_TRAINED = false` and `PERMANENT_HEAD_TRAINED = false`; current cleanup uses measurements. | Train or license task-appropriate weights with verified commercial-use rights and validate permanent-mark protection. |
| P0 | QC can overstate verification | Advanced workflow previously allowed absent skin metrics to pass and described unmeasured “200% edges.” | Missing data must remain incomplete; report actual analysis resolution; inspect per-face contours and real exported crops. First fixes implemented in this audit. |
| P0 | Multiple portrait/mask pipelines | Automatic planning uses YuNet/selfie mattes; legacy portrait-region rendering uses `aura-portrait` with Haar-based detection and measured regions. | One canonical per-photo/per-person analysis and one transformation/provenance contract consumed by every operator and overlay. |
| P0 | Failed edits can still enter delivery | The one-click workflow's edit-error branch explicitly says it will export the last saved reversible recipe. | Hold failed/unverified edits out of an automatically approved final set, surface the affected photos, and make any fallback delivery explicit. Successful file writing must not hide an editing failure. |
| P1 | Under-eye decomposition | Stronger brightness/chroma correction can still confuse socket shape, pigment, makeup and illumination. | Separate lower-lid protection, orbital shading, pigment/cast and reflection masks; edit only the intended component. |
| P1 | Small/slender skin regions | Full-frame/crop inference and geometric exclusions can lose narrow strips beside hair, eyes, jewellery or tika. | High-resolution person/face tiles, boundary uncertainty, labels for accessories and cosmetic/ceremonial pigment; region-specific fail-safe behaviour. |
| P1 | Colour accuracy across mixed light | `smart_edit.rs` already compares gray-pixel/gray-edge estimates and neutral evidence, with scene guards. Learned WB/tone heads elsewhere remain untrained. | Calibrated confidence, mixed-illuminant local estimates, chart-based camera validation and an explicit style/correction separation. |
| P1 | Reference-match scope/resolution | `paired_reference.rs` currently caps both source/reference at 16 MP and searches nine global controls at a 384-pixel preview before verification. | Bounded support for these 30.1 MP files, robust correspondence and semantic style matching across different photographs. |
| P1 | Representative acceptance data | Synthetic tests and four flagged portraits cannot establish broad parity. | Identity-separated benchmark spanning complexions, poses, lighting, makeup, cameras and failure modes; blinded professional review. |
| P2 | Deployment/performance | Native CPU paths and some GPU code exist; operator presence does not establish accelerated parity or responsive 100% previews. | Measure per-stage latency, memory, cancellation, tile overlaps and CPU/GPU numerical agreement on production-size inputs. |
| P2 | Workflow clarity | Several paths expose similar controls but use different analysis or verification. | Consistent overlays, per-region edited/protected/skipped status, propagated QC warnings and selective reprocessing. |

Model cards are historical evidence, not the sole source of truth. Several old cards still describe the phase-06 detector as untrained while the automatic pipeline now uses actual YuNet assets. Similarly, `docs/colour-management.md` says real camera profiles are absent, but current `aura-raw/src/colour/profile.rs` includes embedded-DNG, bundled bench, published-camera-table and generic fallback sources. Documentation and capability badges need to reflect the path actually used.

## 7. Why the user's four examples failed

The following is a diagnosis of the observed artifact class and the relevant AURA risks, not a claim that an Evoto algorithm was recovered.

| Example | Visible problem | Likely mechanisms to test | Appropriate corrective target |
|---|---|---|---|
| 989A2698 | Skin-like ledge/band between chin and neck | A source shadow treated as a defect; low-frequency spill across chin; mismatched face/body correction; mask interpolation. | Preserve coherent chin shadow and gradient direction; compare face/neck corrections along the same boundary. |
| 989A2699 | Under-eye colour looks detached from surrounding skin | A separate eye patch with a different chroma target; too much lightening; incompatible feathering; eyelid/socket mixed into the correction. | A single continuous, curvature-aware delta field with eyelid protection and limited chroma change, measured against the same local lighting. |
| 989A2812 | Extra rim around nose and chin | Smoothing, skin unification or patch repair crossing an anatomical edge; donor from a different surface. | Protect nose-wing structure and chin/neck separation; reject donors/corrections crossing those boundaries. |
| 989A2843 | Local contour/shine transitions and protected details need attention | Several individually small edits combine; broad exclusions leave untreated islands, while aggressive repair flattens other transitions. | Audit the combined final stack, protect bindi pigment and expression contours, reduce local excursions without filling all shadows. |

The latest existing v36 guards address several of these concrete examples. They do not prove that unseen side profiles, closed eyes, darker complexions, heavy makeup, glasses or different lighting will be safe. Coordinate-specific fixes should remain regression fixtures; they must not become the entire product strategy.

## 8. Canonical analysis AURA should implement

**Independent proposal.** Create one immutable analysis object keyed by source hash, orientation, analysis/model versions and processing domain. It should contain person IDs, face boxes, dense landmarks, coarse segmentation, fine labels, alpha/boundary uncertainty, region quality, illumination estimates and protected marks. Every recipe mask and UI overlay references this object or embeds its reproducible result. Do not obtain a second contradictory face mask from a different detector inside a later renderer.

Coordinate domains must be explicit: sensor coordinates, oriented-original coordinates, face-crop coordinates, analysis-grid cells, render pixels and output-crop pixels. Store forward/inverse transforms, pixel-centre conventions and alignment residuals. Test rotation, mirroring, crop, perspective and resampling. A one-pixel shift at a feature is a real defect, even when the mask is otherwise accurate.

Recommended fine labels include face skin, body skin, eyes/sclera/iris/pupil, upper/lower eyelids, brows/lashes, lips/oral cavity/teeth, nostril openings, facial hair, scalp hair, jewellery/glasses/cloth, makeup and ceremonial pigment. Labels that cannot be reliably inferred from a single still image should carry uncertainty or require manual/temporal evidence. Dense landmarks alone do not constitute semantic segmentation.

AURAs mask-combination rule should be operation-specific:

```text
coverage(op, pixel) = semantic_support
                   × person_ownership
                   × confidence_allowance
                   × anatomical_allowance(op)
                   × artist_mask

coverage = 0 wherever a hard protection for this operation is present
```

An eye-brightening operation may intentionally touch sclera; a skin-smoothing operation must not. Under-eye skin must not be excluded merely because it is darker. A universal “skin or not skin” mask cannot express all these choices.

## 9. Fine edges and small skin selections

**Independent proposal.** Start with coarse person segmentation, then evaluate each sufficiently large face crop at a resolution appropriate to the smallest relevant feature. Revisit uncertain boundary tiles using their original-resolution image pixels and neighbouring label evidence. Do not obtain a 30 MP mask by enlarging a 256-pixel prediction and calling the result perfect.

Use an inner confirmed-skin region, outer confirmed-non-skin region and uncertain trimap. Refine alpha using colour, image edges and semantic constraints. A guided filter is useful but cannot reliably distinguish skin from a similarly coloured background without semantics. Keep internal protected holes separate from exterior background; a bindi or eye is not an outside-colour sample for the face contour.

Use a signed distance to the actual boundary for feathering. The feather should stay within allowed support for repair/smoothing, retain hard-protected cores, and vary with local resolution and confidence. Avoid mask dilation that grows skin over hair, nostrils, jewellery or background. Conversely, hard geometric exclusion capsules can create untreated bands; replace them with confidence-aware anatomical boundaries where reliable parsing is available.

For tiny strips, the safe response is a smaller local correction or a reported skip, not invented skin pixels. Record minimum supported feature width, effective mask resolution and uncertainty. Near transparency, flyaway hair, veils or motion blur, mixed pixels require conservative alpha treatment; they are not wholly skin.

## 10. Blemishes and permanent details

**Independent proposal.** Replace “dark/red compact region” as the final decision with a trained temporary-defect detector conditioned on the local skin and anatomical region. Keep the existing measured detector as a conservative fallback and as a source of candidate proposals, not ground-truth labels.

Training labels should distinguish acne/pimples, temporary irritation, flaking, makeup texture, moles, freckles, scars, facial hair, pores, ceremonial pigment, highlights, nose shadows and expression folds. The permanent/temporary distinction is often ambiguous in one photo. Use user protection or trusted cross-frame correspondence when available; do not claim certainty based on darkness alone.

Each repair needs a clean donor or a bounded non-generative reconstruction. Donors must belong to the same person and similar anatomical/illumination region, avoid other defects, match local texture scale and direction, and avoid crossing eyes, lips, nostrils, chin or hair boundaries. Fit low-frequency tone/chroma separately from transferred high-frequency texture. Reject repairs that duplicate a visible pattern, erase a structural edge, create a gradient discontinuity or exceed a change budget.

The polished-wedding profile currently enables dark-mark removal and does not keep all freckles. That is a user aesthetic preference, not proof that every dark mark is temporary. UI and saved reports must distinguish stronger cosmetic cleanup from medically or temporally classified lesions. Preserve specifically protected moles/marks regardless of generic cleanup strength.

## 11. Smoothing and frequency separation

**Independent proposal.** Work in a defined linear/perceptual domain, explicitly split low-, medium- and high-frequency content at scales tied to face size and output pixels, and modify only the bands required for the measured defect. Test pore preservation separately from noise preservation; keeping high-frequency energy can preserve noise while losing real texture, or replace real texture with synthetic-looking grain.

Normalized selected-pixel convolution, already present in AURA, avoids averaging eyelashes into adjacent skin. It should be combined with semantic barriers and local affine shading preservation so that neighbourhood imbalance near a mask boundary does not turn real facial slope into a corrective band. Tone evenness and smoothing should not flatten the nose, lower eyelid, nasolabial fold or chin shadow.

Full-size tiles need sufficient halo for the **composed** filter reach, not just the largest individual radius. Compare tiled and untiled output. Do not evaluate texture only on an eroded skin interior: that deliberately excludes the region where most reported edge artifacts occur. Keep interior texture, feature protection and boundary measurements separate.

Frequency separation and dodge/burn remain opt-in or policy-gated for suitable single-person portraits. Group portraits, tiny faces and uncertain anatomy should use simpler bounded corrections and report why advanced work was skipped.

## 12. Under-eye logic

**Independent proposal.** The under-eye region needs at least five distinguishable components: lower-lid rim/volume, orbital/socket shading, pigment/cast, fine wrinkles/skin texture and eye makeup. Dark circles are not equivalent to the darkest pixels below the eye, and the cheek is not always under the same illumination.

Estimate a smooth local illumination trend from comparable neighbouring skin while excluding sclera, lashes, makeup and specular highlights. Correct the residual darkness in log-luminance using one continuous field, preserve the photographed socket gradient, and change chroma only where a measured pigment/cast residual supports it. The existing scalar exposure approach is a useful safeguard; it should be driven by better masks and a separately adjustable eyelid-protection amount.

Validate continuity along the entire lower-lid/cheek boundary. Inspect both eyes independently and account for pose, closed eyes and directional lighting. No edge should become a painted patch just because a global average skin-shift metric remains small. Keep before/after crops and the actual applied delta for human review.

## 13. Dodge and burn without anatomical damage

**Independent proposal.** Solve a bounded exposure field in log luminance at micro and medium scales. Preserve coherent structural gradients and high-frequency texture; penalize abrupt changes in the correction field and propagation across protected anatomy. A conceptual objective is:

```text
minimize data-fit-to-unwanted-residual
       + smoothness-of-exposure-field
       + structural-gradient-change penalty
subject to exposure bounds, protected pixels and person/region ownership
```

This is a design target, not an assertion that Evoto uses this objective. Micro irregularity, medium blotches, facial sculpting and global exposure should have separate budgets. Report their cumulative effect. Relighting should not automatically remove the natural cheek-to-neck or nose-wing shadow.

## 14. White balance and technical colour correction

**Independent proposal.** Perfect white balance for every arbitrary photo is not a well-defined or always recoverable target. An unknown coloured object under unknown coloured light is ambiguous, and mixed lighting cannot generally be fixed by one temperature/tint pair. The meaningful goal is a defensible estimate, preserved intent and a clear uncertainty response.

The existing gray-pixel/gray-edge consensus, face exclusion, bright-neutral witnesses and scene guards are useful. Improve them through confidence calibration on real camera data, not by forcing all skin toward a fixed peach or pink value.

Recommended processing and validation:

1. Decode the actual sensor data where supported; identify black levels, white levels, CFA, orientation and camera/illuminant metadata. Distinguish an embedded JPEG from a full RAW decode.
2. Use the correct source camera transform. Validate table lookup, exact camera variants, illuminant conventions and fallback provenance. Published colour matrices are a useful baseline but not a complete camera rendering profile or measured local capture calibration.
3. Estimate illumination from multiple neutral/edge cues and metadata. Reject clipped, fluorescent, metallic, coloured-light and unreliable skin/makeup samples. Preserve known warm/cool scene intent when evidence is weak.
4. Detect spatially different illuminants. Offer bounded local subject/background corrections rather than an extreme global compromise; keep transition fields continuous.
5. Set exposure and highlight/shadow treatment from subject and scene evidence. Preserve white dresses, gold jewellery, red fabric, specular highlights, low-key scenes and high-key backgrounds.
6. Apply chromatic adaptation and gamut handling in a documented order. Avoid repeated display/linear conversions, double profiles, double tone curves or double white balance.
7. Evaluate a calibrated colour target under known light with colour differences, neutral chroma error, clipping and repeatability. Evaluate skin preferences separately from chart accuracy.
8. Apply aesthetic grade after technical correction; allow an approved reference to deliberately remain warm, muted or contrasty.

For a shot with no credible neutral evidence, report uncertain WB and preserve the camera setting or the user's approved reference. A forced correction is worse than a correctly reported uncertainty.

## 15. Colour/style matching versus same-photo matching

Current `paired_reference.rs` solves a valuable narrower problem: matching the finished version of the **same** photo through AURA's renderer with protected recipe fields and full-output comparison. It hashes the reference, checks source dimensions, bounds the search, measures per-face errors and does not invent a quality percentage. Those safeguards should remain.

Its 16,000,000-pixel cap excludes the 4480 × 6720 source photos in this job. Raising a constant alone would ignore multiple simultaneous RGB/f32 buffers and full-resolution candidate costs. Add a memory-budgeted/tiled evaluation path, bounded decoding, cancellation and colour-managed reference handling first. Preserve same-crop/orientation checks and baseline-hash protection.

Cross-photo style matching needs a different contract: semantic regions and their distributions, robust tone curves, hue-dependent mapping, subject/background balance and skin-preserving constraints. An arbitrary reference can differ in subject, camera, crop and lighting. Pixel MAE is not a valid cross-photo appearance metric. Never move facial structure or borrow another person's skin texture just to reduce a style loss.

No paired full-resolution Evoto output with exact settings was available to this audit. Existing AURA exports are not Evoto references. Exact parity claims must wait for an authorized reference set, documented profiles/slider values, version identity and aligned output comparison.

## 16. Quality control that can catch the reported mistakes

The current source contained a concrete false-pass path: `retention.is_none_or(...) && shift.is_none_or(...)`. Both skin measurements could be absent and the result still became a pass. The stage also rendered at `Screen(1600,1200)` while its notes used “100%” and “200%” language. The old edge note described mask design, not measured output quality. Those claims were broader than the evidence.

This audit implements these changes in `crates/aura-app/src/advanced_retouch.rs` and the new `advanced_retouch/quality.rs`:

- Record actual QC width/height and `measured`/`incomplete`/`not_measured` status.
- Require finite texture, skin-shift and clipping measurements and contour coverage for every detected face before reporting a pass.
- Add per-face face/adjacent-neck measurements of abrupt new RGB seams and strongly weakened source edges. These are explicitly pilot heuristic warnings, not a trained anatomical diagnosis.
- Gate an automatic QC correction on measured improvement without worsening available texture/colour checks, clipping or contour warnings. Missing/non-finite replacement readings cannot count as proof.
- Include a clipping-increase bound in the advanced QC result.
- Replace unsupported magnification and identity-guarantee language with actual scope and review notes.
- Read older or incomplete reports conservatively in the UI and display the analysis dimensions.

The new contour measurement compares local RGB differences inside the face box and 25% of its height below it. A constant RGB offset does not itself create a new seam. It flags a sufficient number of abrupt new transitions in previously smooth regions, or substantial loss of strong source edges. Its thresholds are engineering pilot settings; they require calibration on labelled real defects. It does not identify an exact nose, eyelid or jaw boundary and does not automatically repair pixels. Dense semantic protection remains a separate P0 requirement.

**Scope limitation:** these QC changes are integrated into the advanced-retouch workflow. The standard `smart_edit::auto_retouch` and one-click delivery workflow do not yet share the complete advanced pixel-QC/report contract. The app-level `aura-qc` service reads stored measurements from other phases; it should not be assumed to have measured every current editable automatic operation. Propagating one common verified artifact/report through all edit/export paths is still required.

A delivery file that decodes, has the right dimensions/ICC and matches its recorded checksum can still contain a bad retouch. Export integrity and portrait appearance therefore need separate results. Neither should be labeled “all verified” when only the first ran.

## 17. Required full-resolution acceptance checks

**Independent proposal.** Build a revision-bound review artifact before final export:

| Check | Measurement and scope | Failure response |
|---|---|---|
| Protected features | Native-resolution RGB/chroma/edge changes in eyes, nostrils, lip border, hairline, ceremonial marks and jewellery, against a baseline with identical global colour. | Revert or weaken the responsible automatic operation; preserve manual choices and report the region. |
| Chin/nose contour | Gradient direction/magnitude, contour displacement and halo width along independently labeled boundaries. | Reject the responsible correction; do not paint the rim away with another broad smooth. |
| Under-eye continuity | Change-field jumps, lower-lid edge retention and local chroma/luma continuity, per eye. | Reduce/withdraw local eye correction; report unavailable anatomy. |
| Interior texture | Multi-scale signal retention, pore continuity, noise/texture discrimination and repeated-donor patterns. | Re-solve at lower strength or withdraw repair. |
| Skin colour | Per-person and per-region change; compare same lighting zones, not a fixed skin target. | Reduce local unification, review mixed light and makeup exclusions. |
| Clipping/gamut | Channel clipping, highlight hue, out-of-gamut behaviour and protected bright fabric/skin. | Revisit foundation or local highlight correction. |
| Coverage/completeness | Each detected face and intended body region has edited/protected/skipped/unavailable status. | Mark incomplete and show the missing regions; do not manufacture a pass. |
| Export | Decode, bit depth, ICC, dimensions, orientation, metadata policy, checksum and manifest. | Exclude failed artifacts from delivery; preserve a diagnostic. |

The review must carry source hash, exact recipe hash, model/engine versions, analysis scale and export settings. Any subsequent recipe or model change invalidates the old appearance verdict. Do not reuse an old QC pass for a new rendering.

## 18. Data and trained-model development

**Independent proposal.** Assemble a consented or properly licensed training/evaluation corpus with source quality, identities and capture conditions documented. Rights to source images, annotations and model weights must each be tracked; an open implementation does not automatically grant commercial rights to a downloaded checkpoint or its training set.

Needed labels: fine facial/body classes, boundary trimaps, temporary defects, permanent/protected marks, cosmetic/ceremonial pigments, occlusion, source illumination and professional repair masks/results. Use multiple frames where possible for permanent-mark correspondence. Keep identities, sessions and near-duplicate frames separated between training and evaluation. Otherwise a system can appear excellent by recognizing the same person/capture in both sets.

Train or adapt specialised heads rather than interpreting an untrained output as confidence. Calibrate confidence against actual errors, including narrow edges. Proposed objectives can combine class loss, boundary/distance-transform loss, landmark consistency and protection penalties. A repair model additionally needs identity/detail consistency and artifact penalties. These are candidate methods to evaluate, not Evoto's recovered losses.

The validation set must include South Asian wedding makeup and tika/bindi/sindoor, red clothing casts, gold jewellery, beards, veils, dark skin under low light, pale skin near clipping, closed eyes, profiles, expressions, motion blur, compression and glasses. Measure worst-case subgroups and failure coverage rather than only one overall mean.

## 19. Benchmark and parity protocol

Use the original flagged four images as a regression set, then add a much broader held-out corpus. A proposed starting design is hundreds of carefully labelled scenes with multiple subjects/complexions/poses, followed by a larger independent acceptance set. This is a planning target, not a claim that such data is already available or that any fixed count guarantees parity.

For an Evoto comparison, record the exact application version, source file, camera profile, WB/tone settings, retouch settings, masks, output profile and dimensions. Use authorized exports only. Compare RAW baseline, AURA technical correction, AURA retouch, and Evoto reference separately. Align geometry without warping away a retouch defect. Different profiles, crops and output transforms can otherwise dominate a pixel score.

Use regional contour error, protected-feature change, multi-scale texture, colour differences on controlled targets, failure rate and completion rate, supplemented by blinded professional review at fit-to-screen and 100% native crops. Pixel equality is meaningful only for exactly aligned identical-domain images; low average error does not establish good retouching or identity preservation. A professional preference score is not a calibrated percentage of “Evoto quality.”

Define release targets before tuning: zero known protected-mark erasures in the acceptance set, specified boundary/colour tolerances, no unreported skips, acceptable runtime/memory, and no regression on the flagged cases. Thresholds should be chosen with annotator agreement, measurement noise and resolution in mind; do not invent universal numerical guarantees.

## 20. Workflow and UI improvements

The useful parity target is a complete, reliable editing workflow. Provide a single-person selector, face/body/feature overlays, add/erase refinement, per-feature protection strength, separate technical colour and style controls, side-by-side and blinking comparison, 100% native crops, per-operation undo and a queue of images that need attention.

For every intended area, show what happened: face skin, forehead beside tika, under-eyes, nose wings, lips, chin/neck, arms/hands and other visible skin. Distinguish “protected” from “missed,” “no correction needed” from “not measured,” and “mask selected” from “pixels changed.” Showing a green face-shaped overlay is not proof that the actual repair did the right thing.

Batch presets should synchronize editing intent while adapting to per-image/per-person measurements. Never copy a skin RGB target between different people or use another face's donor texture. Keep style preferences such as polished wedding, maximum cleanup and permanent-mark protection explicit and reproducible.

Culling needs its own evaluation for blur, closed eyes, expression, duplicates and coverage; it is not solved by portrait smoothing. RAW support needs per-camera/codec validation. Lens correction needs real lens profiles. Tethering, generative repair, cloud syncing and mobile parity are separate projects, not implicit consequences of adding portrait controls.

## 21. Implementation order and acceptance gates

| Phase | Concrete work | Done only when |
|---|---|---|
| A: truthful verification | Fix false passes; record QC scale; add contour readings; reject unproven QC corrections; conservative legacy-report parsing. | Unit/UI/native-pair tests pass and limitations are shown. Implemented in this audit; validation below. |
| B: one analysis contract | Unify automatic and legacy portrait masks, transforms, person ownership and model provenance; expose exact renderer masks. | Overlays and rendered selections agree under rotations/crops; every consumer uses the same source identity. |
| C: trained parsing | Integrate a rights-verified dense face/part parser, calibrate confidence, add high-resolution uncertain-edge refinement. | Held-out boundary/protection metrics and actual marked-up crops pass, including tiny skin strips. |
| D: trained defects | Temporary-defect/permanent-mark classification and region-safe repair/donor validation. | False removals and artifact rates meet predeclared targets; uncertain marks remain reviewable. |
| E: under-eye and light | Separate eyelid/socket/pigment/makeup logic, continuous local fields, structural-gradient constraints. | No detached patches or lost anatomy on flagged and held-out cases. |
| F: colour/reference | Calibrate camera/WB pipeline; mixed-light handling; memory-bounded 30 MP paired matching; semantic cross-photo style transfer. | Controlled colour targets and approved references meet defined tests without skin/identity regressions. |
| G: all-path delivery QC | Shared revision-bound pixel QC in standard, advanced, batch and export workflows; native-resolution crop checks and review queue. | A missing measurement cannot become a delivery pass; all changed renders invalidate prior QC. |
| H: performance/product parity | Profile CPU/GPU, memory, tile boundaries, caching, cancellation, recovery, presets and UI responsiveness. | Representative batch measurements and fault-injection tests pass on target hardware. |

Do not schedule a trained model as “finished” merely because an ONNX adapter compiles. Required work includes annotation, rights verification, model selection, operator support, training/adaptation, calibration, held-out evaluation, deployment and monitoring. A reliable time/cost estimate requires those inputs and the target hardware; a fixed completion promise would be speculative.

## 22. Source map for the next implementation work

| Concern | Current files |
|---|---|
| Face detection and coarse skin | `crates/aura-vision/src/portrait.rs`, `skin.rs`, `skin_body.rs`, `skin_orientation.rs` |
| Legacy semantic-looking measured regions | `crates/aura-portrait/src/parse.rs`, `face.rs`, `regions.rs`, `skin.rs`; `crates/aura-render/src/portrait.rs` |
| Automatic planning and settings | `crates/aura-app/src/portrait_auto.rs`, `portrait_auto/finish_policy.rs`, `retouch_settings.rs`, `smart_edit.rs` |
| Current feature protection | `portrait_features/eye_guard.rs`, `nose_openings.rs`, `jaw_guard.rs`, `anatomy_edges.rs`, `ceremonial_marks.rs` |
| Defect/repair planning | `portrait_features/deep_blemish.rs`, `portrait_auto/body_cleanup.rs`; `crates/aura-retouch/src/ops.rs` |
| Skin/matte/under-eye rendering | `crates/aura-render/src/retouch_skin.rs`, `retouch_matte.rs`, `retouch_undereye.rs`, `retouch_heal/` |
| Technical colour | `crates/aura-raw/src/colour/`, `crates/aura-render/src/colour.rs`, `tonemap.rs`, `crates/aura-app/src/smart_edit.rs` |
| Reference work | `crates/aura-app/src/paired_reference.rs`, `reference_style.rs`; `crates/aura-style/src/` |
| Pixel QC changed here | `crates/aura-app/src/advanced_retouch.rs`, `advanced_retouch/quality.rs`, `advanced_retouch/measure.rs` |
| Existing QC service | `crates/aura-qc/src/checks/`, `crates/aura-app/src/qc_commands.rs`, `state.rs` |
| Batch/export integration | `crates/aura-app/src/one_click_commands.rs`, `delivery_commands.rs`; `crates/aura-export/` |
| QC UI | `ui/src/ipc/advancedRetouch.ts`, `ui/src/components/develop/AdvancedRetouch.tsx` |

## 23. What is still unknown or unfinished

The following have not been recovered or implemented as Evoto-equivalent systems: exact private skin/face models, training data, blemish semantics, proprietary protection masks, exact healing/filter algorithms, exact white balance, full semantic cross-photo colour matching, feature-complete UI behaviour, and output parity. Resource names and ONNX Runtime strings cannot substitute for these missing facts.

This audit does not add Evoto proprietary assets, replace AURA with Evoto code or create an exact branded UI clone. It adds independently written QC code, a reproducible read-only installation-audit script, regression coverage and this report. The key remaining investment is validated analysis and defect understanding, with unified mask/provenance handling and full-resolution QA; stronger automatic smoothing alone would repeat the failures already reported.

## 24. Validation addendum

Local evidence is under `.work-checks/evoto-audit-2026-10-18`. Previously delivered photographs and the original RAW files were not overwritten by this audit.

| Validation | Result |
|---|---|
| `cargo test --locked -p aura-app --lib -j 2` | 247 passed, 0 failed, 2 explicitly ignored. Includes new false-pass, invalid-input, seam/erased-edge and correction-acceptance regressions. |
| Advanced-retouch and native workspace UI tests | 40 passed across 2 files. Includes incomplete/legacy QC reports and missing face/texture evidence. |
| TypeScript check | Passed. |
| Application Clippy check | Blocked by an existing denied `indexing_slicing` lint at `crates/aura-app/src/smart_edit.rs:690`. That line was already in the dirty workspace and was not changed by this audit. Existing warnings also mention `jaw_guard.rs` and the bundled-profile panic documentation. This check is not reported as passing. |
| Production UI build | Passed. Existing large-bundle warning remains; it does not establish a responsive performance target. |
| Desktop build | Passed with the existing MinGW warning about multiple non-default manifests. Updated executable: `target/desktop-launch/debug/aura-desktop.exe`. |
| Desktop smoke check | Updated application launched; the bundled UI loaded, the native IPC bridge responded, and `activity_status` returned no active work. No photo edit/export command was invoked. |
| Python audit helper | Syntax check and actual read-only installation run passed: 2 binaries, 100 effect modules, 97 opaque config carriers. |
| Installed Evoto binary integrity | Final independent SHA-256 checks matched the initial main and worker hashes. Both files remained unchanged. |
| `git diff --check` | No whitespace errors; Git reports existing CRLF normalization notices in other changed files. |
| Native contour review | One ignored-by-default integration test explicitly run and passed over four previously generated native full-resolution before/after pairs. |

Native contour results at **4480 × 6720** pixels:

| Photo | Sampled pairs | New seam pairs | Strong source-edge pairs | Weakened source-edge pairs | Pilot warning |
|---|---:|---:|---:|---:|---|
| 989A2698 | 240,900 | 5 | 31,661 | 3,446 | No |
| 989A2699 | 224,952 | 7 | 11,633 | 297 | No |
| 989A2812 | 286,296 | 22 | 36,941 | 577 | No |
| 989A2843 | 229,080 | 23 | 25,284 | 1,434 | No |

These use the existing v36 native renders in `.work-checks/portrait-2843-v36/native-release`, not a new Evoto export. The test verifies readable/aligned full-size inputs and measurement availability. “No pilot warning” is **not** proof of perfect skin, a trained defect-classifier result, a new visual approval of every pixel, or Evoto parity. Synthetic positive controls verify that a conspicuous painted seam and erased source edge can trigger the measurement. A broader labelled false-positive/false-negative study remains necessary.

Reproduce this review with:

```powershell
$env:AURA_CONTOUR_REVIEW_ROOT = (Resolve-Path '.work-checks/portrait-2843-v36/native-release').Path
$env:AURA_CONTOUR_REVIEW_OUT = Join-Path (Resolve-Path '.work-checks/evoto-audit-2026-10-18').Path 'native-contours.json'
cargo test --locked -p aura-app --test portrait_contour_review -j 2 -- --ignored --nocapture
```

Deep REA/Ghidra inspection imported the worker into a private static-analysis snapshot but **timed out at the default 330,000 ms startup/analysis deadline**. The saved `worker-ghidra.evidence.json` contains `provider_timeout`, the target SHA-256, NTFS admission, Ghidra 12.1.4 identity and startup diagnostics. A successful import log is not completed function analysis. It reports no loaded companion libraries and no PDB information; consequently neither complete type recovery nor original source is established.

A second attempt used the documented reduced-analysis configuration, disabling `Decompiler Switch Analysis`, `Decompiler Parameter ID`, `Stack` and `Function ID`, and extending startup to 450,000 ms. It also returned `provider_timeout` (452,017 ms total command duration). Its full diagnostic is preserved as `worker-ghidra-reduced.evidence.json`. The two distinct profiles remain separate evidence records. No successful decompilation, complete native function inventory or algorithm recovery is claimed. Further deep inspection needs a suitably scoped/persistent analysis project and additional analysis time; it remains unfinished. Decompiler failure is not substituted with invented algorithm descriptions.
