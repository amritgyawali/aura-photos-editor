# AURA: 20 advanced retouching capabilities

Research date: 2026-09-30. Code baseline: `d01cacd`.

Twenty prioritized retouching capabilities across five selected professional reference products. This is a documented feature study and AURA target specification, not an independent product ranking or implementation-completion report.

The five products were chosen to cover manual precision, frequency workflows, specialist plugins and automated portraits. The order is an AURA implementation priority, not a sales ranking or a measured quality leaderboard. Product names in each entry are positively documented examples; omission does not mean a product lacks the feature. Vendor documentation establishes advertised functionality, not independently tested image quality.

## Five reference products

- **Adobe Photoshop:** Manual healing, cloning, tonal brushes and compositing. Manual tools are not evidence of automatic face-aware processing. [Photoshop Healing Brush](https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/healing-brush-tool.html); [Photoshop Clone Stamp](https://helpx.adobe.com/photoshop/desktop/repair-retouch/heal-clone/retouch-images-with-the-clone-stamp-tool.html); [Photoshop Dodge and Burn](https://helpx.adobe.com/photoshop/desktop/repair-retouch/adjust-light-tone/dodge-or-burn-image-areas.html).
- **Affinity:** Editable frequency separation and precision texture work. Current Affinity Pixel studio documentation; not limited to the older Photo 2 edition. [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained).
- **Retouch4me:** Specialist automated portrait and studio cleanup plugins. A suite of separate tools; access depends on the plugins and workflow purchased. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [Retouch4me Frequency Separation](https://retouch4.me/products/retouch-plugins/116?lng=en).
- **Evoto:** Automated skin, complexion and portrait adjustments. The manual documents high/low-frequency sliders; this does not establish a general layer-stack editor. [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/); [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching).
- **PortraitPro:** Portrait-specific skin, eyes, makeup and lighting controls. Automatic batch processing is documented for Studio Max; Photoshop Smart Filter integration for Studio and Studio Max. [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

## What the code audit establishes

- `ui/src/components/develop/RetouchPanel.tsx` has Off/Light/Natural/Polished presets, per-person strength, protected-feature review and texture reporting. Its operation labels cover blemishes, under-eye correction, tone evening and shine.
- A search of `ui/src` finds RetouchPanel and MicroRetouchPanel in their definitions and component tests, but no editor mount points. The existing panels do not establish a connected end-user Retouch workflow.
- Tauri retouch/micro commands are registered in `ui/src-tauri/src/main.rs`; application passes exist in `crates/aura-app/src/retouch_commands.rs` and `micro_commands.rs`.
- `crates/aura-render/src/retouch.rs` contains donor-patch healing and frequency-based tone processing. `crates/aura-brain-photo/src/local/dodgeburn.rs` contains evening and face-zone lighting logic. Neither proves an editable frequency-layer or brush-authoring interface.
- `crates/aura-retouch/src/micro/` contains eyes, teeth, hair, clothing, glare and reference borrowing foundations.
- Historical module comments describe placeholder models and synthetic fixtures. Those comments alone cannot establish current model availability. Loaded-model readiness and actual real-photo routing must be checked before automatic features are called complete.
- The earlier five-portrait enhancement/export checks did not establish quality for these twenty retouching workflows. This research pass does not claim a new retouch image benchmark.

**Status key:** Foundation = related code exists, with integration/quality work remaining. New workflow = the requested end-to-end tool was not established by this bounded audit. Neither status means production-complete. P0 = first usable core, P1 = portrait completeness, P2 = specialist/creative additions.

## Twenty feature specifications

### 1. Blemish and acne removal

**Priority:** P0 · **AURA status:** Foundation

**Documented examples:** Retouch4me, Evoto, Adobe Photoshop. Heal / blemish removal / Healing Brush. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching); [Photoshop Healing Brush](https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/healing-brush-tool.html).

**AURA target:** Remove temporary spots using nearby texture, with editable target and donor regions.

**Controls:** Sensitivity, size, strength, protect mark, brush override.

**Current gap:** Detector and donor-patch renderer exist; editor controls and real-photo quality still need verification. Related code: [crates/aura-retouch/src/blemish.rs](../crates/aura-retouch/src/blemish.rs).

**Acceptance check:** No spill outside the mask; reject poor donors and keep original pixels recoverable.

### 2. Precision healing and clone brush

**Priority:** P0 · **AURA status:** New workflow

**Documented examples:** Adobe Photoshop, Affinity. Healing Brush / Clone Stamp; Affinity high-band cloning. [Photoshop Healing Brush](https://helpx.adobe.com/photoshop/desktop/repair-retouch/clean-restore-images/healing-brush-tool.html); [Photoshop Clone Stamp](https://helpx.adobe.com/photoshop/desktop/repair-retouch/heal-clone/retouch-images-with-the-clone-stamp-tool.html); [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained).

**AURA target:** Provide user-selected sampling, source previews and repeatable strokes.

**Controls:** Heal or clone, source point, radius, hardness, flow, aligned sampling.

**Current gap:** Automatic donor healing is not an interactive clone/heal brush. Related code: [crates/aura-render/src/retouch.rs](../crates/aura-render/src/retouch.rs).

**Acceptance check:** Source coordinates remain correct after zoom/crop/rotation; undo and export reproduce the strokes.

### 3. Editable frequency separation

**Priority:** P0 · **AURA status:** Foundation

**Documented examples:** Affinity, Retouch4me, Evoto. Affinity editable bands; Retouch4me two/three-band Photoshop output; Evoto frequency sliders. [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained); [Retouch4me Frequency Separation](https://retouch4.me/products/retouch-plugins/116?lng=en); [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/).

**AURA target:** Expose tone and texture independently through a reversible frequency operation.

**Controls:** Radius, low/high amount, band preview, per-band mask; optional mid band later.

**Current gap:** Band decomposition exists; editable frequency-layer authoring has not been established. Related code: [crates/aura-render/src/bands.rs](../crates/aura-render/src/bands.rs).

**Acceptance check:** Neutral settings reconstruct the original within float tolerance; preview and full-size export use equivalent physical radii.

### 4. Micro dodge and burn

**Priority:** P0 · **AURA status:** Foundation

**Documented examples:** Adobe Photoshop, Evoto, Retouch4me. Dodge/Burn brushes; Evoto Even with Dodge & Burn. [Photoshop Dodge and Burn](https://helpx.adobe.com/photoshop/desktop/repair-retouch/adjust-light-tone/dodge-or-burn-image-areas.html); [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/); [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Even small luminance variations while retaining the skin's fine detail.

**Controls:** Auto amount, dodge/burn brush, flow, luminance range, mask.

**Current gap:** Evening logic exists; a dedicated manual/automatic Retouch workflow is not verified. Related code: [crates/aura-brain-photo/src/local/dodgeburn.rs](../crates/aura-brain-photo/src/local/dodgeburn.rs).

**Acceptance check:** Measure texture and hue drift; prevent highlight clipping and dark halos.

### 5. Macro dodge and burn / portrait relighting

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Evoto, Retouch4me, PortraitPro. Sculpt with Dodge & Burn / Portrait Volumes / Image Relighting. [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/); [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

**AURA target:** Refine broad facial illumination with separate controls from micro evening.

**Controls:** Contour intensity, highlight/shadow amounts, lighting preview, local override.

**Current gap:** Face zones and light-direction analysis exist; editor integration requires work. Related code: [crates/aura-brain-photo/src/local/dodgeburn.rs](../crates/aura-brain-photo/src/local/dodgeburn.rs).

**Acceptance check:** No geometric movement; stable results across a person's frames without flattened facial structure.

### 6. Selective skin color correction

**Priority:** P0 · **AURA status:** Foundation

**Documented examples:** Evoto, Retouch4me, Affinity. Skin tone controls; low-frequency color work. [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/); [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained).

**AURA target:** Correct local casts and redness while keeping skin luminance and texture separately controllable.

**Controls:** Temperature, tint, hue/chroma, redness, luminance lock, sampled reference.

**Current gap:** Tone evening and global white balance are starting points, not complete skin-specific color controls. Related code: [crates/aura-retouch/src/evening.rs](../crates/aura-retouch/src/evening.rs).

**Acceptance check:** Skin-mask boundaries blend cleanly; hair, lips and background retain their intended colors.

### 7. Face-to-body complexion matching

**Priority:** P1 · **AURA status:** New workflow

**Documented examples:** Evoto. Unify Body Complexion and separate face/body complexion tools. [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/).

**AURA target:** Match face, neck and body color under inconsistent lighting.

**Controls:** Reference region, affected regions, color amount, luminance amount.

**Current gap:** No complete face-to-body reference-matching workflow was established in the inspected surface. Related code: [crates/aura-retouch/src/evening.rs](../crates/aura-retouch/src/evening.rs).

**Acceptance check:** Respect different illumination; compare measured color changes inside and outside chosen skin regions.

### 8. Texture-preserving skin smoothing

**Priority:** P0 · **AURA status:** Foundation

**Documented examples:** Evoto, PortraitPro. Textured Smoothing / ClearSkin. [Evoto Skin Retouching manual](https://support.evoto.ai/portrait-retouching-module-skin-retouching/); [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

**AURA target:** Smooth uneven tone with independently constrained texture changes.

**Controls:** Smoothing, texture retention, pore preview, protected regions.

**Current gap:** Texture guard and tone-evening engine exist; real-portrait acceptance remains necessary. Related code: [crates/aura-retouch/src/texture_guard.rs](../crates/aura-retouch/src/texture_guard.rs).

**Acceptance check:** Check actual rendered texture plus visual realism at 100%; band-energy alone does not prove pore fidelity.

### 9. Shine and oily-highlight reduction

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Retouch4me. Mattifier. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Reduce distracting specular shine without flattening all highlights.

**Controls:** Amount, highlight threshold, feathering, brush refinement.

**Current gap:** Shine-reduce operation exists; selective control and real-photo behavior need verification. Related code: [crates/aura-render/src/retouch.rs](../crates/aura-render/src/retouch.rs).

**Acceptance check:** Retain intended catchlights and luminous highlights; no gray patches.

### 10. Under-eye shadow and bag correction

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Evoto, Retouch4me. Dark-circle / Dodge & Burn retouching. [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching); [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Lighten local under-eye shadows while preserving lower-eyelid texture and depth.

**Controls:** Left/right strength, shadow lift, area refinement.

**Current gap:** Under-eye analysis and renderer exist; manual bounds and review controls need integration. Related code: [crates/aura-retouch/src/undereye.rs](../crates/aura-retouch/src/undereye.rs).

**Acceptance check:** Keep eyelid edges intact and avoid a bright strip across the cheek.

### 11. Wrinkle and fine-line softening

**Priority:** P1 · **AURA status:** New workflow

**Documented examples:** Evoto, Affinity. Wrinkle retouching and independent tone/texture repair. [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching); [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained).

**AURA target:** Offer selective reduction of distracting lines with an adjustable retained amount.

**Controls:** Region brush, reduction amount, texture retention.

**Current gap:** Generic smoothing is not a dedicated wrinkle-selection workflow. Related code: [crates/aura-retouch/src/texture_guard.rs](../crates/aura-retouch/src/texture_guard.rs).

**Acceptance check:** Retain expression and avoid erasing all facial lines; verify at original resolution.

### 12. Eye redness and vessel cleanup

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Retouch4me, PortraitPro. Eye Vessels; eye cleaning and red-eye correction. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

**AURA target:** Separate sclera cleanup from flash red-eye correction.

**Controls:** Redness, sclera amount, red-eye target, per-eye mask.

**Current gap:** Sclera/iris analysis exists; pupil red-eye handling and dedicated controls remain to verify. Related code: [crates/aura-retouch/src/micro/eyes.rs](../crates/aura-retouch/src/micro/eyes.rs).

**Acceptance check:** Do not bleach the sclera or spill into eyelids and iris.

### 13. Iris, lash and catchlight refinement

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** PortraitPro, Retouch4me. Eye Enhancement / Eye Brilliance. [PortraitPro features and editions](https://www.anthropics.com/portraitpro/); [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Add controlled existing-eye detail and catchlight adjustment.

**Controls:** Iris detail, lash contrast, existing-catchlight intensity, left/right controls.

**Current gap:** Iris and catchlight guards provide a starting point, not the complete tool. Related code: [crates/aura-retouch/src/micro/eyes.rs](../crates/aura-retouch/src/micro/eyes.rs).

**Acceptance check:** No invented iris detail in default enhancement; catchlight geometry remains stable.

### 14. Natural teeth whitening

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Retouch4me. White Teeth. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Reduce distracting tooth color casts with bounded local luminance changes.

**Controls:** Warmth reduction, brightness, strength, tooth mask.

**Current gap:** Tooth-locus corrections exist; selection and individual controls need integration. Related code: [crates/aura-retouch/src/micro/teeth.rs](../crates/aura-retouch/src/micro/teeth.rs).

**Acceptance check:** Preserve tooth shading and gum/lip colors; avoid clipped white teeth.

### 15. Glasses reflection reduction

**Priority:** P2 · **AURA status:** Foundation

**Documented examples:** PortraitPro. Reduce Reflections in Glasses. [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

**AURA target:** Reduce recoverable glare; allow aligned reference-frame repair when valid.

**Controls:** Glare mask, reduction, optional reference, alignment preview.

**Current gap:** Glare and sibling-frame borrowing modules exist; broad single-image glare recovery is not established. Related code: [crates/aura-retouch/src/micro/glare.rs](../crates/aura-retouch/src/micro/glare.rs).

**Acceptance check:** Do not claim to recover fully obscured detail without evidence; compare edges and alignment.

### 16. Stray hair and flyaway cleanup

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** Retouch4me, Affinity. Stray Hairs; sampled high-frequency texture repair. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [Affinity frequency separation workflow](https://www.affinity.studio/blog/frequency-separation-explained).

**AURA target:** Clean selected flyaways while retaining the hairline and intentional strands.

**Controls:** Sensitivity, thickness, keep/remove brush, donor preview.

**Current gap:** Measured stray-hair analysis exists; difficult backgrounds and editor brush control need validation. Related code: [crates/aura-retouch/src/micro/hair.rs](../crates/aura-retouch/src/micro/hair.rs).

**Acceptance check:** Protect hairline and foreground strands; no repeated background patterns.

### 17. Lip and digital makeup refinement

**Priority:** P2 · **AURA status:** New workflow

**Documented examples:** PortraitPro. Digital Makeup. [PortraitPro features and editions](https://www.anthropics.com/portraitpro/).

**AURA target:** Apply user-chosen lip, cheek and eye color through editable feature masks.

**Controls:** Lip color/opacity, blush, eye makeup, per-region masks.

**Current gap:** No complete makeup workflow was found in the inspected Retouch surface. Related code: [ui/src/components/develop/RetouchPanel.tsx](../ui/src/components/develop/RetouchPanel.tsx).

**Acceptance check:** Creative controls remain separately chosen; masks track features and preserve texture.

### 18. Clothing lint and wrinkle cleanup

**Priority:** P2 · **AURA status:** Foundation

**Documented examples:** Retouch4me. Fabric / Dust. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins).

**AURA target:** Separate small lint repairs from broad fabric-fold smoothing.

**Controls:** Lint sensitivity, fold amount, garment mask, manual repair.

**Current gap:** Clothing-issue analysis exists; broad fabric wrinkle removal is not proven. Related code: [crates/aura-retouch/src/micro/clothing.rs](../crates/aura-retouch/src/micro/clothing.rs).

**Acceptance check:** Preserve seams, logos and fabric weave without stretched textures.

### 19. Studio backdrop cleanup

**Priority:** P2 · **AURA status:** New workflow

**Documented examples:** Retouch4me, Adobe Photoshop. Clean Backdrop; manual cloning. [Retouch4me retouching plugins](https://retouch4.me/retouchplugins); [Photoshop Clone Stamp](https://helpx.adobe.com/photoshop/desktop/repair-retouch/heal-clone/retouch-images-with-the-clone-stamp-tool.html).

**AURA target:** Remove backdrop spots and small folds with subject-aware repair.

**Controls:** Backdrop mask, dust size, fold amount, edge protection.

**Current gap:** Garment cleanup does not establish backdrop cleanup; needs its own region workflow. Related code: [crates/aura-retouch/src/micro/clothing.rs](../crates/aura-retouch/src/micro/clothing.rs).

**Acceptance check:** Keep hair and subject edges; avoid flattening intended background gradients.

### 20. Batch retouching with individual overrides

**Priority:** P1 · **AURA status:** Foundation

**Documented examples:** PortraitPro, Evoto. PortraitPro Studio Max batch mode; Evoto batch adjustments. [PortraitPro features and editions](https://www.anthropics.com/portraitpro/); [Evoto portrait retouching](https://www.evoto.ai/features/portrait-retouching).

**AURA target:** Apply a selected retouch preset across photos with per-photo and per-person review.

**Controls:** Preset, selected photos, per-person strength, review queue, cancel/resume.

**Current gap:** Project passes and per-identity strength contracts exist; the full editor workflow and mask recalculation need validation. Related code: [crates/aura-app/src/retouch_commands.rs](../crates/aura-app/src/retouch_commands.rs).

**Acceptance check:** Recompute image-specific masks and donors; never copy raw pixel coordinates across photographs.

## Shared implementation requirements

These are proposed AURA engineering requirements, not claims about competitors' internal algorithms.

1. A coherent Retouch workspace with sections for Skin, Light, Color, Details and Cleanup; an Auto pass plus manual overrides.
2. Every operation has an editable mask, amount, enabled state and history entry. Support brush/erase refinement, mask overlays and a held before/after comparison.
3. Reliable face/skin/detail masks are prerequisites for automatic targeting. Expose model readiness; permit manual regions when automatic targeting is unavailable.
4. Use AURA's linear Rec.2020 pipeline, retain originals and preserve manual decisions when re-running automation.
5. Reuse frequency, healing and dodge/burn foundations through one persisted operation model. Document any frozen-contract changes in an ADR before implementation.
6. Frequency separation needs explicit low/high-band controls, maskable operations and identity reconstruction at neutral settings. Existing band filtering is not enough to claim this workflow.
7. Retouch edits must survive save/reopen and render consistently at preview and full export resolution. Radius, masks and donor sampling must transform with crop, orientation and image dimensions.
8. Start with preserving the subject's existing features. Keep intentional makeup controls separate from automatic cleanup.
9. Validate automatic results per face; show skipped/uncertain operations in the review queue. Do not use made-up confidence scores.
10. Measure memory, latency and export throughput on the supported hardware; cancel/resume must not corrupt recipes.

## Build sequence

- **Foundation:** mount the existing panels, wire commands to current image/project state, verify model readiness and masks, and establish operation persistence plus undo/redo.
- **P0 core:** features 1, 2, 3, 4, 6, 8. Deliver manual targeting alongside supported automation. This includes the four explicitly requested families: blemish cleanup, color correction, dodge and burn, frequency separation.
- **P1 portrait completeness:** features 5, 7, 9, 10, 11, 12, 13, 14, 16, 20. Add dedicated detail controls and image-specific batch recomputation.
- **P2 specialist tools:** features 15, 17, 18, 19.

## Real-photo acceptance plan

Use the five existing real-person portrait fixtures in `.work-checks/portrait-review/originals/` for an initial smoke run. They are not sufficient to establish population-wide quality. Add consented or appropriately licensed close-up, group, low-light, textured-skin, glasses, flyaway-hair and fabric examples covering a range of skin tones and ages.

For each implemented tool: preserve original hashes; save before/after full-resolution crops and the operation recipe; check changed-pixel bounds; inspect pore/edge fidelity; test zero strength, undo/redo, reopen and native export. For frequency separation, test neutral reconstruction and band-local changes. For brush tools, test zoom/crop/rotation coordinates. For batches, verify separate mask/donor computation per photo and correct per-person overrides. Subjective professional-retouch quality requires human review beyond these numerical checks.

## Related records

- [100-feature comparison](photo-editor-feature-roadmap.md)
- [Implementation record](photo-editor-implementation.md)
- [Machine-readable twenty-feature specification](advanced-retouching-top20.json)

This commit adds research and specifications only. It does not implement or enable the twenty tools.
