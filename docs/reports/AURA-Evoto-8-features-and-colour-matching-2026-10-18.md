# AURA: Evoto 8 feature audit and implementation specification

Date: 2026-10-18. Scope: the installed Windows desktop application, public desktop documentation, and the current AURA source tree. This report continues the earlier 7.3.5 audit; it does not replace its historical binary evidence.

## Findings and implementation status

Evoto's reference-image colour workflow is materially broader than AURA's existing folder-based style analysis and same-photo matcher. The priority is a single edited reference applied to different new photographs, independently adjustable tone, colour and warmth, repeatable settings, regional protection, and honest before/after measurements. Copying the original editing parameters from a finished image is generally underdetermined. Matching appearance is the achievable product requirement.

The installed executable now reports **8.0.0.0**, is 237,784,976 bytes, and has SHA-256 `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`. It was inspected in its existing session at `F:\evoto\Evoto.exe`. The selected image was `989A2698.CR3`. Library, editing panels, colour matching, masks, portrait groups, background, clothing, pets, crop, creative effects, manual healing and AI Lab were opened or enumerated through Windows accessibility. Screenshots and accessibility trees are retained under `.work-checks/evoto-8-feature-research-2026-10-18`.

**A visible control establishes an available interface, not its accuracy.** Controls in a scroll area can appear in an accessibility tree while outside the viewport. Individual model quality, every slider endpoint, all camera compatibility and all image categories have not been tested. Export opened a free-trial offer rather than the export settings. No trial was activated and no paid export or AI Lab generation was run. Public export specifications are documentation evidence in this report.

The user subsequently requested implementation of all feature families and increase/decrease sliders. The implementation ledger at the end distinguishes completed work from the remaining specification. A matching label, a registered model filename or an algorithm sketch is not counted as a completed feature. Identical Evoto rendering has not been established, and its private model weights, training data and complete rendering sequence remain unknown.

## Evidence and documentation conflicts

The desktop curriculum has 39 lessons covering workspace, colour, portrait face/body, background, clothing, pets, crop, tethering and automation. Its lesson pages were collected individually, alongside specialist support pages, with source URLs and response hashes in `official-document-index.json`. Mobile and video articles are not silently treated as Windows features. [Desktop curriculum](https://academy.evoto.ai/learn/evoto)

Two conflicts require explicit handling:

* The live 8.0 interface places **AI Color Adjustments** and **Masking** on separate sidebar buttons; older documentation describes finding them inside Color. The observed layout is authoritative for this installation.
* The Academy personalized-look lesson suggests 10–20 references. The specialist support page specifies 200+ images per selected training domain, and distinguishes Lite creation from full training. Do not promise the smaller count as the current training minimum. These are also different tasks from matching one finished reference. [Academy lesson](https://academy.evoto.ai/learn/evoto/color-adjustments/personalized-ai-look), [specialist requirements](https://support.evoto.ai/personalized-ai-look/)

Prior decompilation covered selected functions of **7.3.5**. Addresses and call graphs from that executable are not automatically valid for 8.0. Current UI observations can corroborate feature families, but do not transfer the old binary analysis to the new executable.

## Observed feature inventory

The following names are observed interface controls, grouped for engineering work. They do not claim every nested option or model branch has been exercised. Detailed captured labels and source URLs accompany this report in the research artifacts.

### Library, organisation and culling

The Library exposes projects, related folders, collections, bin, ratings, Photos/Stories, filters, metadata, face previews, blur and closed-eye indicators, Perfect Shot, Photo Cluster and Smart Culling. Workflow and Asset Hub are also visible. AURA already has import, projects, ratings, culling, burst grouping and review/export mechanisms. The remaining work is to compare the actual culling decisions, surface reasons, improve people and burst review, and validate linked editing across changing poses. The current inspection used one existing Evoto image, so no comparative culling benchmark was performed.

Documented organisation also includes grid/loupe/survey views, flags, colour labels and Lightroom catalogue import. Settings documentation describes preview/GPU preferences, export defaults, cache location, language, keyboard shortcuts and Favorites. These were researched rather than individually changed. [Library](https://academy.evoto.ai/learn/evoto/getting-started/library-workspace-overview), [settings](https://academy.evoto.ai/learn/evoto/getting-started/keyboard-shortcuts-and-settings)

### Technical colour controls

The live Color panel exposes histogram with RGB/Lab readouts, Real-Time Color Adjustments, Profile, Filters, Basic, White Balance, Temperature, Tint, Exposure, Contrast, Brightness, Highlights, Shadows, White, Black, Texture, Clarity, Dehaze, Vibrance and Saturation. Further groups are Level, Curves, HSL, Color Grading, Selective Color, Detail, Grain, Lens Corrections, Transform Correction and Color Calibration.

AURA already has many corresponding develop parameters and native rendering paths. Required work is an end-to-end control audit: UI range and units, serialization, renderer sensitivity, reset, undo, preview/export agreement, and migration of older recipes. Similar slider names do not establish identical mathematical responses. Camera profiles, demosaic, highlight handling and output transforms can make identical nominal settings look different.

### AI colour, matching and personal looks

Observed AI Color Tools include Auto Color Corrections and Multi-Image Consistency with separate Overall Color (Basic) and Background Color entries. AI Style offers Color Looks and Color Match. The matching panel shows Upload References, Create Reference, Quick Mode, Recommended/My References, reference management and reference thumbnails. Its upload hint lists JPEG/JPG/TIFF/PNG and dimensions from 512 to 12,000 pixels. Quick/Control mode choices were opened. An existing selected recommended reference exposed an Extreme preference, Amount and Masking Adjustments; the full preference enumeration was not recovered.

The specialist guide documents global Amount/Tone/Color controls, semantic regional refinement, references organised into groups, batch synchronization and reusable presets. It also says applying a match resets preceding colour edits, which must be considered when comparing screenshots and recipes. AURA should make replacing versus adding a look explicit and preserve an undoable baseline. [AI Color Match](https://support.evoto.ai/feature-introduction-ai-color-match/)

Personalized AI Look is a separate documented capability: Lite creation uses XMP or a personal Evoto colour preset; full training uses larger edited sets and domain-specific sources. It includes preview/refinement, format compatibility and task states. AURA's existing statistical look profiles and Lightroom learning infrastructure are starting points, not proof of equivalent learned adaptation. [Personalized looks](https://support.evoto.ai/personalized-ai-look/)

### Masks and regional selection

The live mask UI exposes Person, Pets, Background, Custom, All Persons, individual person selection, mask editing, Add/Subtract, Elements, Smart and Manual selection. The opened person-area chooser includes Entire Person, Skin, Face, Facial Skin, Neck, Body Skin, Eyebrows, Eyes (Whites), Iris and an Apparel tab. Additional choices below that chooser's viewport were not individually inspected. Basic colour, curves, HSL, grading, selective colour and detail groups also appear for masks.

The specialist guide additionally documents gradients, brush/luminance selection, semantic/interactive/quick smart selection, edge adjustment and replacement/addition during synchronization. These are interface-level capabilities, not evidence that masks are always correct. AURA needs manual refinement alongside learned selection. [Masking](https://support.evoto.ai/feature-introduction-masking/)

### Portrait selection and defect removal

The live portrait panel supports category selection and individual/all editing, plus a master slider. Blemish Removal contains separate freckles/acne controls, face mole removal, face shine, dark circles, eye bags, glasses glare, nostril cleanup, lip wrinkles/flakes, double chin and beard protection. Facial wrinkle controls distinguish forehead, eleven lines, eye, nasal, smile, cheek, marionette and perioral lines. Body Refinement includes blemishes, neck wrinkles, armpit touch-up, stretch marks and tattoos; some controls were disabled for this image.

AURA has multiscale spot healing, permanent-mark/ceremonial-mark guards and editable retouch operations. Its current deterministic spot evidence is not a trained acne-versus-mole classifier. The task classifiers, proposal confidence and local donor validation need separate evaluation. A dark boundary at the nostril, jaw or eye socket must not be treated as a spot merely because it is darker than adjacent skin.

### Skin, tone and texture

Observed facial tools include Even Dodge & Burn, Sculpt Dodge & Burn with feature/contour controls, Textured Smoothing, Frequency Separation and Skin Softening. Body Skin has its own Even, Frequency Separation and Softening. Skin Texture exposes glossiness, texture and clarity. Skin Tone exposes AI face/body complexion unification, body complexion options, a tone picker, temperature, tint, radiance, rosy complexion and tan-line removal.

AURA's existing boundary and under-eye fixes are useful but do not supply dense anatomical parsing. Keep the user's existing automatic policy: **Dodge & Burn and Frequency Separation only when there is one sufficiently supported portrait**. Cataloguing Evoto's body tools does not authorize weakening that automatic policy. Manual targeted controls can remain independently available.

### Face structure, expression, teeth and eyes

Observed reshape groups cover face shape/ratio, eyebrows, nose, mouth, symmetry and head pose. Face controls include temples, cheekbones, jaw, size, V-shape, width and chin proportions, with left/right control where offered. Expression tools include gummy smile, gentle smile and smile generation. Teeth tools include flaws, whitening, alignment and Pretty Teeth. Eyes include brightness, glasses glare, white enhancement, red eye, symmetry, iris correction and catchlights.

AURA's existing local warp and enhancement operations do not establish learned 3D face or expression generation parity. Geometry must be implemented with stable landmarks, occlusion handling, local support and background protection. Retouch should not introduce reshape implicitly. Expression/teeth generation requires a separately validated model and identity checks; changing a slider label is insufficient.

### Makeup, hair, hands and feet

Makeup exposes suites, highlights, contour, eyebrows, eyes and lips, along with eyebrows, eyeshadow, eyelashes, eyeliner, contacts, blush, lipstick, contour and face decorations. Hair controls include part line, top/side volume, hairline, stray hair, smoothing, shine, white-hair darkening and hair colour. Hands expose Even Dodge & Burn, hand veins and reshape. Feet expose foot veins.

AURA has local colour, texture and retouch primitives that can support portions of these workflows. Fine lip/eye parsing, hair matting and dedicated hand/foot analysis remain separate dependencies. Jewelry, henna, tika, eyelashes and permanent marks need explicit protection. A makeup-colour adjustment must not spill onto teeth or sclera.

### Body proportions

Full Body Reshape exposes AI Reshape and individual head, body, height, upper-body length, neck width/length, arms, breasts, waist, hips and leg controls. These require articulated geometry and occlusion-aware warps. A face box cannot reliably locate every limb. Implementation must preserve jewelry, fabric structure and nearby people and must retain reversible geometry. Existing AURA elliptical warps are partial primitives, not equivalent body analysis.

### Background, clothing and pets

Background tools observed: distractions removal, clean backdrop, unified lighting, colour banding removal, enhancement, backdrop changer, sky replacement and links into AI Lab. Clothing tools: overall/fine/deep wrinkle cleanup, lint, sweat stains, edge smoothing and moiré removal. Pet tools: leash/stray-fur removal and left/right eye brightening, colour enhancement and catchlights.

These are different material and object tasks. Skin healing cannot safely stand in for fabric reconstruction or fur cleanup. Background replacement requires subject alpha and handling of veils, hair, gaps, reflections and retained objects. Clothing changes must protect print, embroidery and seams. Pet operations require species-appropriate detection. AURA's subject/sky/object selection and cleanup infrastructure can be reused, but each missing effect still requires a real renderer/model and acceptance cases.

### Crop, effects and manual tools

Crop/Rotate exposes subject/face location, AI Locate, subject location, vertical/eye alignment, rotation and horizon correction. Creative Effects exposes Lens Blur, Glow and Post-Crop Vignetting. Manual healing was opened in Spot Healing Brush, Patch and Clone Stamp modes. Brush size/hardness, source/destination and clone opacity/mode controls are visible.

AURA already supports crops, geometry and manual retouch primitives. Lens blur requires a validated depth/occlusion estimate and hair-aware compositing; a flat background blur is not equivalent. Glow requires controlled highlight spread and predictable scene-linear composition. Crop-relative vignette coordinates must follow the final crop and rotation.

### AI Lab, capture, automation and delivery

AI Lab's observed catalogue includes background fusion, generative expansion, AI sky replacement, grass fill, smart removal, people removal, clothing moiré removal, golden blonde hair, strong-glare removal, face-shadow removal, belly slimming, old-photo restoration, detail enhancement and liquify-background repair. Batch Edit and History are visible. No generation was executed during this inspection.

Wired/wireless tethering and watched-folder processing are documented desktop workflows; camera hardware was not connected for this audit. Presets, batch synchronization and import/export belong in the same workflow plan, with resumable jobs, clear per-image failures and final verification. [Wired capture](https://academy.evoto.ai/learn/evoto/tethered-shooting/wired-tethered-shooting), [wireless capture](https://academy.evoto.ai/learn/evoto/tethered-shooting/wireless-tethered-shooting), [automation](https://academy.evoto.ai/learn/evoto/auto-import-export/auto-import-export-setup), [import/export](https://academy.evoto.ai/learn/evoto/getting-started/import-export-basics)

## AURA source findings that change the implementation plan

| Area | Actual current evidence | Required change |
|---|---|---|
| Folder references | `aura-core/contract/look.rs` sets `MIN_REFERENCES = 8`. `reference_style.rs` measures a folder and stores aggregates and lighting buckets. | Add a distinct single-image mode; retain statistical minimums for gallery training. |
| Reference formats | `aura-look/source.rs` recognises a WebP extension, but `decode` only branches to PNG or JPEG. TIFF is not implemented in this reference path. | Validate actual codecs; do not advertise extension recognition as decoding support. |
| Reference colour | Measurements assume sRGB pixels. This path does not visibly transform an embedded reference ICC profile or orient pixels using EXIF. | Colour-managed decoding and orientation normalization before measuring arbitrary edited references. |
| Folder fitting | The per-photo command tests three scaled initial proposals through the renderer. It does not run the broader coordinate-descent routine automatically. | More robust bounded fitting and metrics that agree with the enabled controls. |
| Lighting | `measure::read_at` currently returns Unknown with zero lighting confidence. The reference analysis uses this reading directly. | Do not imply useful scene-specific matching from these buckets until populated and tested. |
| Same-photo fit | `paired_reference.rs` requires matching dimensions/crop, uses pixel correspondence and caps 16 MP. | Keep paired fitting separate from different-scene style transfer; remove the resolution limitation only with a bounded-memory implementation. |
| Native skin | `skin.rs` uses the bundled six-class Selfie Multiclass model, person crops and guided refinement. | Add anatomically detailed face parsing and uncertainty; coarse face/body classes cannot identify eyelids, nostrils or lip boundaries precisely. |
| Other masks | `local_mask_commands.rs` already supports native masks and face regions. `ai.rs` supports subject, sky and object models conditionally. Older general semantic/matting heads have untrained flags. | Consolidate capability reporting and segmentation artifacts. Do not describe all AURA selection as untrained or all facial regions as learned. |
| Spot repair | `deep_blemish.rs` explicitly uses multiscale proposals with structural exclusions and editable heals. | Add trained task evidence where justified, preserving reliable deterministic repair and donor checks. |
| Retouch controls | AutoRetouchSettings and native operation controls already contain many strengths, texture/tone and skin-selection parameters. | Add convenient grouped/master controls and ensure changing a setting updates the intended saved operations. |
| Quality | Regional eye/nose/mouth/chin checks and recipe-bound reports already exist. | Add full-resolution boundary sampling, colour-match coverage and source/profile/model bindings; a preview pass is not proof of perfect export. |
| Reference persistence | Existing analysed references live below `cache_root/reference-styles`. | Durable reference library should survive clearing disposable preview caches. |

The two separate selection paths are particularly important. The presence of a newer six-class person model does not turn the older twenty-class heuristic module into a trained twenty-class parser. Likewise, a named blemish ONNX file in `models` does not establish a trained classifier.

## Reference colour matching: independent design

### Input domains and modes

Provide four distinct workflows:

1. **One edited reference:** approximate its visible colour treatment on different new photos. No same-subject or same-crop requirement.
2. **Original/finished pair:** registered, corresponding pixels for fitting a style residual. Measure alignment before using pixel error.
3. **Gallery style:** several consistent references, scene/lighting coverage and uncertainty. Do not pretend a single photograph identifies a photographer's whole style.
4. **Settings/preset import:** supported XMP/CUBE fields with explicit units and unsupported-field reporting. A LUT does not encode RAW WB, masks or blemish repair.

A finished JPEG contains already-applied illumination, camera rendering and artistic changes. Reference warmth is therefore an appearance target, not the original photographer's Kelvin setting. Technical RAW gains and rendered-image colour adaptation require different contracts.

### Colour-managed input and correction

Decode under byte/pixel/allocation budgets. Normalize EXIF orientation, identify bit depth and colour profile, and convert reference/target descriptors to a common colour-managed space. Unknown profiles should be reported, not silently interpreted as a known wide gamut. Preserve original precision for rendering even when the analysis uses a proxy.

Correct technical exposure/WB before applying an aesthetic residual. Use as-shot RAW gains as a candidate, reliable neutral patches and clipped-pixel exclusions, and confidence-weighted evidence. Avoid inferring WB from a predominantly red bridal outfit or warm backdrop. Mixed light should remain locally modelled or be marked uncertain. Preserve intentional lighting instead of forcing every shadow neutral.

### Semantic, content-aware measurements

Match corresponding region classes rather than overall frame averages. Example: a red outfit occupying 70% of the reference should not make a green park target red. Build robust tone quantiles and chroma/hue distributions for skin, hair, clothing and background; exclude uncertain boundaries and clipped/specular samples from fitting. Normalize class weights so class area alone does not dominate style.

Preserve per-person variation. Do not move every face to the same absolute skin colour. Compare skin residuals within comparable illumination and keep makeup and facial-feature protection distinct. If a region is absent in either image, report no comparison for it. Never report zero error for an unmeasured class.

Initially use interpretable bounded exposure/curve/WB/HSL/grading adjustments through AURA's actual renderer. Later add a regularized colour transform or LUT residual if the data supports it. Tone curves must remain well behaved, hue transitions continuous and gamut handling explicit. These are proposed AURA techniques; Evoto's exact internal method is not recovered.

### Objective and checks

The fitting objective should combine region-conditioned tone and chroma differences with penalties for clipping, implausible skin shifts, edge halos and excessive parameter changes. A full-image RGB error is inappropriate for different photographs. A descriptive appearance distance is not a percentage of similarity to Evoto.

Evaluate every accepted candidate through the production renderer. Compare the final candidate against the same baseline and the requested slider components. Sample native-resolution boundaries and compare full-frame clipping statistics before saving. Bind the result to source hash, recipe, colour pipeline, reference hash and mask/model version. Cancelled or stale work must not overwrite a later manual edit.

### Adjustable controls and saved state

| Control | Required behaviour |
|---|---|
| Amount | Scale the entire matched treatment; zero restores its baseline. |
| Tone | Independently change exposure/contrast/curve contribution. |
| Colour | Independently change chroma/grading contribution. |
| Warmth/WB match | Change the reference's colour-cast contribution, separate from source technical WB. |
| Face/body skin, hair, clothes, background | Independent region amounts where selection is supported; show missing coverage. |
| Preserve skin/eyes/lips/teeth | Explicit protection with visible selection overlay and manual refinement. |
| Link/unlink | Share strengths or adjust individual regions. |
| Reset/Undo | Restore the correct saved baseline, including after reopening. |
| Save look / Apply to selection | Reuse intent, recomputing scene-dependent parameters on each new photo. |

Never multiply already-scaled amounts on every slider move. Store a stable baseline and fitted proposal, then derive the new recipe from those. Include slider values in history and cache identity. Zero must be a usable removal action even if earlier settings were stronger. A visible slider must control real saved parameters and the final render, rather than only changing a UI number.

## Skin selection and the reported artifacts

The four reported photos remain regression subjects: `989A2698` chin/neck band, `989A2699` under-eye separation, `989A2812` nose/chin edges and `989A2843` under-eye/highlight transitions. Their original RAWs, unretouched render and approved reference crops should be compared in the same colour domain.

Required analysis is a per-person set of class probabilities, landmarks/contours, occlusion, feature exclusions, signed distance and confidence. Use the entire skin support for analysis, but create an operation-specific safe interior for smoothing or healing. Small valid skin islands should not be discarded merely for being small. Equally, increasing mask coverage must not bridge a nostril, hairline or necklace.

For an operation with support radius r, require support inside the selected skin and retain structural gradients. Guided upsampling alone cannot invent missing facial semantics. Do not feather corrections outward across boundaries: taper the correction inward. Donor search needs the same surface, similar illumination, no protected features and enough texture support.

Separate under-eye pigmentation, coherent socket shadow, bags and makeup. Use bounded exposure/chroma correction from a trustworthy nearby reference while retaining original texture and gradient direction. A uniform cheek-colour fill causes the detached patch the user identified. Under-chin shadow is normally structural; avoid flattening it into an extra skin-like ledge. Validate changed pixels around mask borders as well as the selected interior.

## Implementation sequence and acceptance gates

| Order | Deliverable | Completion evidence |
|---|---|---|
| 1 | Single-reference matching with independently adjustable controls and stable baseline | Different images/dimensions work; zero/removal and repeated adjustment do not compound; cancellation and stale edits are safe; renderer output changes. |
| 2 | Durable reference library, colour-managed input and additional formats | Reference survives cache cleanup; ICC/orientation fixtures; large-reference allocation limits; truthful format picker. |
| 3 | Semantic regional matching and manual mask refinement | Region coverage report, red-dress/green-background stress case, per-person skin preservation and native edge inspection. |
| 4 | Detailed face parsing and trained defect proposals | Held-out nose/eyelid/jaw/tika cases, skin-tone and lighting diversity, uncertainty and operation-specific support tests. |
| 5 | Grouped portrait controls and preset synchronization | Reversible parameter changes, no D&B/frequency outside the user's automatic policy, no manual-edit overwrite. |
| 6 | Hair, garments, body geometry, pets, depth blur and remaining effects | Dedicated actual algorithms/models, representative inputs and full-resolution visual review per family. |
| 7 | Tethering, workflow automation and broader delivery parity | Hardware compatibility runs, interrupted-transfer recovery, duplicate prevention, reproducible export manifests. |

Priorities reflect this user's problems, not a claim about implementation time. An independent software system with all these effects requires models, data, UI, rendering and testing across multiple work streams. Adding placeholder controls would make the system less trustworthy.

## Verification protocol

For every feature record source photo/hash, camera/profile, operation settings, crop, model/pipeline version, before/final outputs and review outcome. Test zero/maximum values, serialization, undo/redo, repeat application, unsupported inputs and manual edits. For colour matching add neutral references, deliberately warm references, mixed light, different subjects/composition, saturated clothing and cross-camera targets. Inspect faces, nose boundaries, under-eyes, jaw/neck, hands, jewelry and hair at native scale.

Controlled Evoto comparisons require its authorized final exports and all relevant settings. The currently watermarked preview is not a full-resolution export reference. Assess technical colour with calibrated targets where available; assess artistic preference with approved references and blind review. Do not claim objective perfection from a statistical histogram score.

## Implementation ledger

Further continuation: [Skin connectivity and brush reliability, 2026-10-19](AURA-skin-connectivity-and-brush-reliability-2026-10-19.md) records a reproduced thin-boundary selection failure, pixel-level connectivity refinement, brush-gesture fixes and their separate verification. This does not change the feature-parity limits below.

At initial report creation this turn had produced research artifacts and source findings; production implementation is being undertaken in response to the user's subsequent request. The final verification addendum below is the authority for what actually shipped in this continuation. All unlisted features above remain inventory/specification or prior partial functionality, not newly completed parity.

<!-- IMPLEMENTATION-ADDENDUM -->

### Implementation continuation: 2026-10-19

The new independent implementation lives in `crates/aura-app/src/color_match.rs`, its `appearance.rs` and `reference_image.rs` submodules, and `ui/src/components/look/ColorMatchPanel.tsx`. Tauri commands connect it to the desktop. This is a new path alongside the older multi-reference folder analysis and same-photo fitter; their existing contracts remain available.

Implemented behaviour:

* Import one JPEG, PNG or TIFF reference, including 16-bit raster input, under byte/dimension/allocation budgets. Read EXIF orientation and convert supported RGB/grayscale ICC profiles into sRGB using independent open-source libraries. Untagged inputs assume sRGB. Transparent and unsupported CMYK/Lab reference inputs are refused explicitly.
* Persist reference descriptors in the catalogue's `setting` table. Disposable preview-cache cleanup does not remove this library. The reference's pixels are not copied into the target render.
* Analyse a different target photo without requiring the same crop, subject, dimensions or camera. Fit reduced pixels derived from the native full decode, so the camera's embedded JPEG does not define the RAW fitting domain.
* Measure whole-image and supported skin, hair, clothing and background regions. Exclude uncertain selection borders from colour statistics. Each supported region contributes a fixed class weight rather than a weight proportional to its image area. Skin is additionally checked per supported person, preventing pooled statistics from concealing a shift on one person.
* Search joint bounded proposals and individual tone/colour axes through AURA's actual renderer. Preserve manual-field protections. Global adjustments are currently fitted using semantic measurements; this does **not** yet provide independently painted regional colour transformations or dense facial parsing.
* Expose **Amount, Tone, Colour and Warmth** sliders from 0–100%. Store the original baseline and fitted proposal. Repeated changes derive from that baseline, and zero amount removes the matched treatment. Native validation rejects non-finite and out-of-range strengths. A later unrelated edit makes the analysis stale rather than allowing it to overwrite newer work.
* Render the baseline and candidate at native resolution before applying. Check clipping, supported skin-region tint/chroma/lightness changes and appearance distance. These are bounded colour checks; they do not certify every pore, eyelash or mask boundary as correct. Unsafe or unavailable results are not saved.
* Match a collection through per-photo analysis/application, progress, cancellation and an individual outcome list. A current session for the same reference reuses its original baseline when strengths change. Failed photos are reported individually; no export is initiated by this action.
* Add an **Adjust strength** control beside every saved native retouch operation. Its absolute 0–100% amount updates the existing operation, preserving its selection, skin matte, donor coordinates, healing options and other settings. Draft values are applied explicitly and can be discarded; changing the number alone does not save an edit.
* Expose the existing native **Stretch-mark softening** operation in the manual Body tool list, including **Mark smoothing reach** and **Skin texture retained** controls. This connects an existing renderer to the UI; it is not a newly trained stretch-mark detector. The manual list now contains 31 tools.
* Give reference matching and saved-operation strengths separate slider rows with full-width tracks and wrapping buttons, so the narrow adjustment column remains usable.

The UI is in Develop under **Reference colour matching**. In the native retouch workspace, expand **Saved retouch operations → Adjust strength**. Existing detailed texture, tone, radius and skin-selection controls remain available when selecting an operation.

Important validation findings were fixed rather than hidden: a small-reference resize had enlarged inputs unnecessarily; zero amount needed to restore absent HSL entries as well as numeric zeros; uniform skin patches can have no populated midtone bucket, so protection now checks shadow, midtone, highlight and chroma statistics; and the real CR3 check exposed the need to derive fitting pixels from the native decode. The final verification results are recorded below after the desktop and RAW checks finish.

Remaining work includes dense anatomical parsing and trained defect classification, independently adjustable regional colour transforms, full reference management, learned style training, calibrated mixed-light WB, and the additional geometry/generative/material/pet/camera features listed above. They are not completed merely because their names appear in the inventory. No identical-output or complete-Evoto-replica claim is made.

Decoder and colour-management API references: [image decoder metadata](https://docs.rs/image/0.25.10/image/trait.ImageDecoder.html), [image resource limits](https://docs.rs/image/0.25.10/image/struct.Limits.html), [moxcms](https://docs.rs/moxcms/0.8.1/moxcms/).

<!-- FINAL-VERIFICATION -->

### Final verification: 2026-10-19

| Check | Result and scope |
| --- | --- |
| Focused native unit tests | **8 passed**, covering independent strengths, exact global restoration at zero, noncompounding application, invalid values, 16-bit inputs, ICC conversion, unsupported/transparency handling and per-person skin guards. This is a focused suite, not the entire repository test suite. |
| Native integration tests | **2 passed**, including the normally ignored real-RAW test explicitly enabled with `989A2698.CR3`. The synthetic test uses different reference/target dimensions and verifies stale manual edits survive. |
| Real CR3 check | At 50% overall match with the other axes at 100%, the internal appearance distance decreased from **5.8173685 to 4.77971**. Native-resolution validation passed; 33 recipe leaves changed. The original 35,170,943-byte RAW retained its BLAKE3 checksum. Work used an isolated temporary catalogue. |
| Reference identity | The RAW test reference was **AURA's own `989A2699_AURA_FINAL.jpg`**, 4480 × 6720 with ICC conversion. It was not an Evoto export. The distance is an internal fit measurement, not a perceptual-quality certification or evidence of Evoto parity. |
| Frontend tests | **59 passed across 6 files**: reference panel (4), collection matching (3), saved strength (2), native retouch workspace (38), PhotoStudio (8) and existing paired reference (4). |
| Frontend build | TypeScript and the Vite production build passed with 195 modules. The existing large-chunk warning remains. A final visual check found inline wrapping of new controls; scoped layout styles now place each slider on its own row. |
| Native lint | Focused Clippy completed successfully with **7 warnings**; it was not warning-free. Warnings include existing code and new naming/function-length findings. |
| Desktop build | Offline, locked desktop build with `custom-protocol` succeeded. The Windows resource-linker multiple-manifest warning remains. An earlier online attempt failed because `index.crates.io` could not resolve; cached offline dependencies allowed the build to complete. |
| Live desktop checks | The updated executable launched, exposed the new reference-list IPC command through the real Tauri bridge, and displayed **Reference colour matching**, all four 0–100 sliders, **Adjust strength** for existing saved operations and the stretch-mark tool. With no draft strength change, Apply remained disabled. Read-only navigation did not run editing or export. |
| Existing delivery | No new production photo export was created for this implementation. Existing catalogue edits and the previous final JPEG delivery were not replaced. |

Evidence is under `.work-checks/evoto-8-feature-research-2026-10-18/`: `color-match-unit-final.log`, `color-match-native-final.log`, `ui-tests-final.log`, `ui-build-final.log`, `color-match-clippy.log`, `desktop-final-build.log`, `desktop-final-smoke.json`, `desktop-colour-match-live.json` and `desktop-retouch-strength-live.json`. The final executable fingerprint and runtime check are recorded in `implementation-verification.json`.

The final executable is a debug build with the packaged frontend, SHA-256 `f67215fdd925e95c23713773fc6a76d329cdec25ac222cd43baa17f59efeb9dc`. The final native bridge check was idle with zero image workers busy. The last read-only snapshot was in the native retouch workspace and contained its existing brush stroke-limit notice; verification did not apply or discard a brush draft. This is not evidence that every existing UI interaction is free of issues.

This continuation completes the reference-colour workflow and adjustable saved-retouch controls described above. It does **not** complete every item in the Evoto inventory, recover Evoto's private algorithms or demonstrate identical rendered photos. Dense skin-boundary/under-eye correctness, all additional feature families and calibrated comparisons against authorized Evoto final exports remain separate acceptance work.

<!-- GENERATED-RESEARCH-APPENDICES -->

## Captured UI control ledger

These are captured control labels, including children outside scroll viewports. Counts are observations, not counts of independently implemented effects.


### Library and culling

Evidence: `initial-ui-tree.json`.

Library, Edit, Evoto, Photo, View, Window, Help, Create New, Home, Workflow, Asset Hub, My Space, Projects, Ungrouped Projects, 989A2698, Related Folders, In 989A2698, New Volume(H:/), editing to aura, Collection, Bin, 1 photo(s) / 1 selected, Photos, Stories, Basic Info, Blur, Closed Eyes, 989A2698.CR3, 0.0 EV, ISO 100, f/2.8, 1/400 sec, 154.00 mm, Filters, Add filters, Add filters to narrow down photos, Faces, Perfect Shot, Photo Cluster, Culling, FAQ, Smart Culling.


### Blemishes

Evidence: `edit-workspace.json`.

Portrait Retouching, Female, Male, Child, Elderly Female, Elderly Male, Indiv., All, Master Slider, Face Refinement, Freckles, Acne, Freckles & Acne, Face Mole Removal, Reduce Face Shine, Dark Circles, Eye Bags, Remove Glasses Glare, Nostril Cleanup, Lip Wrinkles & Flakes, Double Chin, Beard Protection, Facial Wrinkles, Forehead Wrinkles, Eleven Lines, Eye Wrinkles, Nasal Wrinkles, Smile Line, L, R, Cheek Wrinkles, Marionette Lines, Perioral Wrinkles, Body Refinement, Body Blemishes, Neck Wrinkles, Armpit Touch-up, Stomach Stretch Marks, Stretch Marks, Tattoos, Blemish Removal, Skin Retouching, Facial Reshape, Facial Expression, Teeth, Eyes, Makeup, Hair, Hands, Full Body Reshape, Feet, AI Lab, Belly Slimming, Strong Glare Removal, Face Shadow Removal, Golden Blonde Hair, Liquify Background Repair.


### Colour

Evidence: `color-panel.json`.

Color Adjustments, Features Reorganized, AI Color Adjustments and Masking are now easier to find on the right bar., AI Color Adjustments, Open Masking, Profile, Filters, Basic, White Balance, Temp, Tint, Tone, Exposure, Contrast, Brightness, Highlights, Shadows, White, Black, Presence, Texture, Clarity, Dehaze, Vibrance, Saturation, Level, Curves, HSL, Color Grading, Selective Color, Detail, Grain, Lens Corrections, Transform Correction, Color Calibration, Histogram, ISO 100, 154mm, f/2.8, 1/400s, R, -, G, B, L, a, b, Real-Time Color Adjustments.


### AI colour and reference matching

Evidence: `ai-color-panel.json`.

Color Adjustments, Profile, Filters, Basic, White Balance, Temp, Tint, Tone, Exposure, Contrast, Brightness, Highlights, Shadows, White, Black, Presence, Texture, Clarity, Dehaze, Vibrance, Saturation, Level, Curves, HSL, Color Grading, Selective Color, Detail, Grain, Lens Corrections, Transform Correction, Color Calibration, Histogram, ISO 100, 154mm, f/2.8, 1/400s, R, -, G, B, L, a, b, Real-Time Color Adjustments, AI Color Adjustments, AI Color Tools, Auto Color Corrections, Apply, Multi-Image Consistency, Overall Color (Basic), Match, Background Color (Bas…, AI Style, Color Looks, Color Match, Supported Formats: JPEG, JPG, TIFF, PNG
Image Dimensions: 512px to 12,000px, Upload References, Create Reference, Quick Mode, Recommended, My References, Wedding 01, Wedding 02, Family, School, Baby, Headshot, Ungrouped.


### Reference matching modes

Evidence: `match-modes.json`.

Color Adjustments, Profile, Filters, Basic, White Balance, Temp, Tint, Tone, Exposure, Contrast, Brightness, Highlights, Shadows, White, Black, Presence, Texture, Clarity, Dehaze, Vibrance, Saturation, Level, Curves, HSL, Color Grading, Selective Color, Detail, Grain, Lens Corrections, Transform Correction, Color Calibration, Histogram, ISO 100, 154mm, f/2.8, 1/400s, R, -, G, B, L, a, b, Real-Time Color Adjustments.


### Masking

Evidence: `masking-panel.json`.

Masking, Semantic Masking, Person, Pets, Background, Custom, All Persons, Create Mask, +Entire Person, Basic, White Balance, Temp, Tint, Tone, Exposure, Contrast, Brightness, Highlights, Shadows, White, Black, Presence, Texture, Clarity, Dehaze, Vibrance, Saturation, Level, Curves, HSL, Color Grading, Selective Color, Detail, Histogram, ISO 100, 154mm, f/2.8, 1/400s, R, -, G, B, L, a, b, Real-Time Color Adjustments.


### Person mask chooser

Evidence: `mask-area-selector.json`.

Entire Person, Skin, Face, Facial Skin, Neck, Body Skin, Eyebrows, Eyes (Whites), Iris, Cancel, Update Mask, Person, Apparel.


### Skin retouch

Evidence: `skin-retouching.json`.

Facial Skin, Even (Dodge & Burn), Facial Features, Facial Contours, Sculpt (Dodge & Burn), Textured Smoothing, Frequency Separation, Skin Softening, Body Skin, Skin Texture, Glossiness, Texture, Clarity, Skin Tone, AI Unify Face Complexion, AI Unify Body Complexion, Unify Body Complexion, AI Body Complexion, Select Skin Tone, Temperature, Tint, Skin Radiance, Rosy Complexion, Remove Tan Lines.


### Face geometry

Evidence: `facial-reshape.json`.

Face Shape, Face, L, R, Temple, Cheekbones, Jaw, Face Size, V-Shape, Jaw Length, Face Width, Chin Taper, Chin Length, Chin Shape, Head Pose, Up / Down, Face Ratio, Eyebrows, Nose, Mouth, Symmetrize.


### Expression

Evidence: `facial-expression.json`.

Gummy Smile, L, R, Gentle Smile, Smile Generation.


### Teeth

Evidence: `teeth.json`.

Teeth Flaws Removal, Teeth Whitening, Teeth Alignment, Pretty Teeth.


### Eyes

Evidence: `eyes.json`.

Brightness, L, R, Remove Glasses Glare, Eye White Enhance, Red Eye Removal, Eye Symmetry, Iris Correction, AI Iris Correction, Catchlights, General, Indoors.


### Makeup

Evidence: `makeup.json`.

Makeup Suite, Highlights, Contour Intensity, Eyebrow Makeup, L, R, Eye Makeup, Amount, Lip Makeup, Eyebrows, Eyeshadow, Eyelashes, Eyeliner, Contacts, Blush, Lipstick, Contour, Face Decorations.


### Hair

Evidence: `hair.json`.

Hair Reshape, Hair Part Line, Top Hair Volume, Side Hair Volume, L, R, Hairline, Hair Perfection, Stray Hairs Removal, Smooth Hair, Hair Shine Enhancement, Hair Color Adjustments, White Hair Blackening, Hair Color.


### Hands

Evidence: `hands.json`.

Even (Dodge & Burn), Hand Vein, Hand Reshape.


### Body geometry

Evidence: `body-reshape.json`.

Try Now, Full Body, AI Reshape, Head, Body, Height, Upper Body, Upper Body Length, Neck Width, L, R, Neck Length, Arms, Breasts, Waist Width, Waist Length, Hips, Leg, Leg Width, Thigh, Calf, Amount, Leg Length.


### Feet

Evidence: `feet.json`.

Foot Veins.


### Background

Evidence: `background-panel.json`.

Background Adjustments, Solid Backdrop Refinement, Distractions Removal, Clean Backdrop, Unify Lighting, Color Banding Removal, Background Refinement, Grass Fill, Try Now, Background Enhancement, Backdrop Changer, Sky Replacement, AI Lab, Smart Removal, People Removal, AI Background Fusion, Generative Expand, AI Sky Replacement, Features Reorganized, Lens Blur is now moved to Creative Effects., Locate Lens Blur.


### Clothing

Evidence: `clothing-panel.json`.

Clothes & Accessories Adjustments, All, Female, Male, Child, Elderly Female, Elderly Male, De-wrinkle Clothing, Amount, Fine Wrinkles Removal, Deep Wrinkles Removal, De-blemish Clothing, Lint Removal, Sweat Stain Removal, Clothing Edge Smoothing, Clothing Moiré Removal, Try it now, Clothing Adjustment, AI Lab.


### Pets

Evidence: `pet-panel.json`.

Pet Retouching, Leash Removal, Apply, Stray Fur Removal, Body, Eye Lightening, L, R, Eye Color Enhancement, Catchlights, General, Indoors, Outdoors, Eyes.


### Crop

Evidence: `crop-panel.json`.

Crop  Rotate, Expand, Crop, Subject, Face, AI Locate, Subject Locate, Person Auto Alignment, Vertical Alignment, Horizontal Eye Alignment, Rotate, AI Horizontal Correction, Auto, Angle, Crop & Rotate, Cancel, OK.


### Creative effects

Evidence: `creative-panel.json`.

Creative Effects, Lens Blur, AI Lens Blur, Glow Effect, Post-Crop Vignetting, AI Lab, Old Photo Restoration, Detail Enhance.


### AI Lab

Evidence: `ai-lab-catalog.json`.

Evoto_pro, AI Lab, 989A2698.CR3, 100%, Batch Edit, Background & Scene, AI Background Fusion, Generative Expand, AI Sky Replacement, Grass Fill, Remove & Repair, Smart Removal, People Removal, Clothing Moiré Removal, Subject Adjustments, Golden Blonde Hair, Strong Glare Removal, Face Shadow Removal, Belly Slimming, Enhance & Restore, Old Photo Restoration, Detail Enhance, Liquify Background Repair, Cancel, Apply, Tools, History, Library, Edit, Evoto, Photo, View, Window, Help, Portrait Retouching, Female, Male, Child, Elderly Female, Elderly Male, Indiv., All, Master Slider, Blemish Removal, Skin Retouching, Facial Reshape, Facial Expression, Teeth, Eyes, Makeup, Hair, Hands, Full Body Reshape, Feet, 1 photo(s) / 1 selected.


### Healing

Evidence: `manual-healing-tools.json`.

Healing Tool, Spot Healing Brush Tool, Size, Hardness, Shuffle Watermark Position, Shuffle, Certain effects will be temporarily hidden when using the Healing Tool., Smart Removal, Try Now, Cancel, OK.


### Patch

Evidence: `manual-patch-tool.json`.

Healing Tool, Patch Tool, Source, Destination, Shuffle Watermark Position, Shuffle, Certain effects will be temporarily hidden when using the Healing Tool., Smart Removal, Try Now, Cancel, OK.


### Clone

Evidence: `manual-clone-tool.json`.

Healing Tool, Clone Stamp Tool, Size, Hardness, Opacity, Mode, Shuffle Watermark Position, Shuffle, Certain effects will be temporarily hidden when using the Healing Tool., Smart Removal, Try Now, Cancel, OK.


## Official document ledger

| Source | Retrieval |
|---|---|
| [auto import export setup](https://academy.evoto.ai/learn/evoto/auto-import-export/auto-import-export-setup) | Collected; URL and response hash recorded |
| [backdrop changer](https://academy.evoto.ai/learn/evoto/background-adjustments/backdrop-changer) | Collected; URL and response hash recorded |
| [background adjustments solid backdrop](https://academy.evoto.ai/learn/evoto/background-adjustments/background-adjustments-solid-backdrop) | Collected; URL and response hash recorded |
| [sky replacement](https://academy.evoto.ai/learn/evoto/background-adjustments/sky-replacement) | Collected; URL and response hash recorded |
| [clothing dewrinkle deblemish](https://academy.evoto.ai/learn/evoto/clothing-accessories/clothing-dewrinkle-deblemish) | Collected; URL and response hash recorded |
| [clothing edge smoothing](https://academy.evoto.ai/learn/evoto/clothing-accessories/clothing-edge-smoothing) | Collected; URL and response hash recorded |
| [ai color looks and filters](https://academy.evoto.ai/learn/evoto/color-adjustments/ai-color-looks-and-filters) | Collected; URL and response hash recorded |
| [ai color match](https://academy.evoto.ai/learn/evoto/color-adjustments/ai-color-match) | Collected; URL and response hash recorded |
| [detail sharpen noise reduction](https://academy.evoto.ai/learn/evoto/color-adjustments/detail-sharpen-noise-reduction) | Collected; URL and response hash recorded |
| [grain and lens corrections](https://academy.evoto.ai/learn/evoto/color-adjustments/grain-and-lens-corrections) | Collected; URL and response hash recorded |
| [masking local adjustments](https://academy.evoto.ai/learn/evoto/color-adjustments/masking-local-adjustments) | Collected; URL and response hash recorded |
| [personalized ai look](https://academy.evoto.ai/learn/evoto/color-adjustments/personalized-ai-look) | Collected; URL and response hash recorded |
| [presence hsl color calibration](https://academy.evoto.ai/learn/evoto/color-adjustments/presence-hsl-color-calibration) | Collected; URL and response hash recorded |
| [white balance and tone](https://academy.evoto.ai/learn/evoto/color-adjustments/white-balance-and-tone) | Collected; URL and response hash recorded |
| [crop types and ai face locate](https://academy.evoto.ai/learn/evoto/crop-and-rotate/crop-types-and-ai-face-locate) | Collected; URL and response hash recorded |
| [rotate and ai correction](https://academy.evoto.ai/learn/evoto/crop-and-rotate/rotate-and-ai-correction) | Collected; URL and response hash recorded |
| [ai lab people smart removal](https://academy.evoto.ai/learn/evoto/getting-started/ai-lab-people-smart-removal) | Collected; URL and response hash recorded |
| [create your account](https://academy.evoto.ai/learn/evoto/getting-started/create-your-account) | Collected; URL and response hash recorded |
| [download and install](https://academy.evoto.ai/learn/evoto/getting-started/download-and-install) | Collected; URL and response hash recorded |
| [edit workspace overview](https://academy.evoto.ai/learn/evoto/getting-started/edit-workspace-overview) | Collected; URL and response hash recorded |
| [import export basics](https://academy.evoto.ai/learn/evoto/getting-started/import-export-basics) | Collected; URL and response hash recorded |
| [keyboard shortcuts and settings](https://academy.evoto.ai/learn/evoto/getting-started/keyboard-shortcuts-and-settings) | Collected; URL and response hash recorded |
| [library workspace overview](https://academy.evoto.ai/learn/evoto/getting-started/library-workspace-overview) | Collected; URL and response hash recorded |
| [presets using managing importing](https://academy.evoto.ai/learn/evoto/getting-started/presets-using-managing-importing) | Collected; URL and response hash recorded |
| [pet body retouching](https://academy.evoto.ai/learn/evoto/pet-retouching/pet-body-retouching) | Collected; URL and response hash recorded |
| [pet eyes](https://academy.evoto.ai/learn/evoto/pet-retouching/pet-eyes) | Collected; URL and response hash recorded |
| [body skin retouching](https://academy.evoto.ai/learn/evoto/portrait-retouching-body/body-skin-retouching) | Collected; URL and response hash recorded |
| [full body reshape](https://academy.evoto.ai/learn/evoto/portrait-retouching-body/full-body-reshape) | Collected; URL and response hash recorded |
| [hair](https://academy.evoto.ai/learn/evoto/portrait-retouching-body/hair) | Collected; URL and response hash recorded |
| [hands](https://academy.evoto.ai/learn/evoto/portrait-retouching-body/hands) | Collected; URL and response hash recorded |
| [ai facial recognition](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/ai-facial-recognition) | Collected; URL and response hash recorded |
| [blemish removal](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/blemish-removal) | Collected; URL and response hash recorded |
| [eyes and makeup](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/eyes-and-makeup) | Collected; URL and response hash recorded |
| [facial expression and teeth](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/facial-expression-and-teeth) | Collected; URL and response hash recorded |
| [facial reshape](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/facial-reshape) | Collected; URL and response hash recorded |
| [skin retouching face](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/skin-retouching-face) | Collected; URL and response hash recorded |
| [skin texture and tone](https://academy.evoto.ai/learn/evoto/portrait-retouching-face/skin-texture-and-tone) | Collected; URL and response hash recorded |
| [wired tethered shooting](https://academy.evoto.ai/learn/evoto/tethered-shooting/wired-tethered-shooting) | Collected; URL and response hash recorded |
| [wireless tethered shooting](https://academy.evoto.ai/learn/evoto/tethered-shooting/wireless-tethered-shooting) | Collected; URL and response hash recorded |
| [feature introduction ai color match](https://support.evoto.ai/feature-introduction-ai-color-match/) | Collected; URL and response hash recorded |
| [feature introduction masking](https://support.evoto.ai/feature-introduction-masking/) | Collected; URL and response hash recorded |
| [personalized ai look](https://support.evoto.ai/personalized-ai-look/) | Collected; URL and response hash recorded |
| [ai lab](https://support.evoto.ai/ai-lab/) | Collected; URL and response hash recorded |
| [skin retouching](https://support.evoto.ai/skin-retouching/) | Collected; URL and response hash recorded |
| [blemish removal](https://support.evoto.ai/blemish-removal/) | Collected; URL and response hash recorded |
