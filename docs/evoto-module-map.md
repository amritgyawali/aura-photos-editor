# Evoto's effect modules against AURA, 2026-10-19

Evoto 7.3.5 ships one configuration folder per effect module under `resources/effect/config/`.
The **names** of those 101 folders are used here as a checklist of what Evoto offers - the same
information as its menus. Nothing inside them, and nothing else in Evoto's installation, was
opened, decompiled or copied: what each module does is taken from its name and from Evoto's
public documentation, and every AURA equivalent is built independently. See
[the published route inventory](evoto-feature-inventory.json) for Evoto's 143 marketing pages.

**Status**: **have** - a native AURA tool does this job; **partial** - part of it, or only by hand;
**build** - missing, and buildable with measurement and classical image processing on this
build; **model** - missing, and needs a generative or learned model this build cannot run;
**policy** - deliberately not automatic in AURA (see [retouch ethics](retouch-ethics.md)).

## Skin

| Evoto module | AURA | Status |
|---|---|---|
| FaceAcne, FaceAcneBaby, BodyAcne | Acne clear, frequency healing, body spots (ADR-0090, 0092) | have |
| FaceDermabrasionHLF, BodyDermabrasionHLF, FacePortraiture, BodyPortraiture | Skin smooth, frequency separation, body smoothing | have |
| AISkinUnify, BodySkinUnify, MultiplePeopleAISkinUnify | Skin uniformity per face, match body to face | have |
| SkinUnifyColorPicker, SkinAdjust, SkinLighten | Skin colour from a sample; skin brightness, warmth, tint settings | have |
| SkinTexture | Texture graft, texture setting | have |
| Degreasing | Shine removal (mattify) | have |
| AIShine | Glow setting, highlight glow | partial |
| NeutralGray, NeutralGrayDB, NeutralGrayBodyEven, NeutralGrayHand | Portrait and micro dodge & burn; hands only as body skin | partial |
| RemoveEyeBag | Dark circles and eye bags (ADR-0094) | have |
| Wrinkle, WrinkleV1, NeckLines | Forehead, crow's feet, smile, under-eye and neck lines | have |
| LipWrinkleRepair | - | build |
| GrowthLine, StretchMark | Stretch marks (ADR-0118) | have |
| RemoveTanLines | Skin colour by hand | partial |
| RemoveBlueVeins, FootRemoveBlueVeins | Reduce visible veins, placed by hand (ADR-0131) | have |
| RemoveStubble, RemoveHair | - | model |
| BeardProtect | Protect facial hair | have |
| DeTattoo | Tattoos are protected from automatic retouch; clone/heal by hand | policy |
| **Strength after the pass** | Retouch strength sliders, one per kind (ADR-0130) | have |

## Face and body shape

| Evoto module | AURA | Status |
|---|---|---|
| AIReshape, FaceSurgery, FaceShaping3D, FaceStereo, FacePlump, EyesSize | Local proportions, explicit ellipse warp, by hand only | partial / policy |
| Liquify, AILiquifyRepairPostProcess | Liquify push, enlarge, shrink, twirl, placed by hand; Undo restores (ADR-0131) | have |
| DoubleChin, RepairBreast, CollarBoneEnhance | - | model / policy |
| PoseRefine, SubPoseRefine | - | model |

## Eyes, mouth, expression

| Evoto module | AURA | Status |
|---|---|---|
| BrightEye, EyeWhiten, EyeRemoveRedBlood, IrisEditing, IrisEditingRedMask | Eye whitening, vessels, iris detail and brightness, eye colour | have |
| RedEyeReduction | Red eye | have |
| PetBrightEye | Pet eye / eye glow fix, placed by hand (ADR-0131); no pet detector | partial |
| TeethWhitenV3 | Teeth whitening | have |
| ToothBeauty, ToothRepairing, RemoveGingiva | - | model |
| NoseFlaw | Blemish repair works on the nose; nose structure protected | partial |
| EyelidGenerator, RemoveDoubleEyelid, SmileMouth, SmileMouth3D, PursedLipsSmile, CloseMouth | - (changes expression or anatomy) | model / policy |
| Makeups, MakeupEnhance | Makeup tool: lip colour and definition, blush, contour, highlight | partial |
| MakeupProtect | - (keep existing makeup out of skin evening) | build |

## Hair and clothing

| Evoto module | AURA | Status |
|---|---|---|
| HairColorAdjust, RemoveWhiteHair | Hair colour (chosen colour, colour brightness) | have |
| HairColorUnify | Unify colour, placed by hand (ADR-0131) | have |
| HairGloss, HairSmooth | Hair shine, hair detail | partial |
| HairSeam | - (fill a hair parting) | model |
| RemovePetHair, WHairSkinCpu | Stray hair heal by hand | partial |
| ClothesBlemishRemoval, ClothRemoveSweat | Fabric lint and marks, sampled stain correction | partial |
| ClothesCrease | Fabric smoothing | partial |
| ClothesEdgeSmooth | - | build |

## Background and scene

| Evoto module | AURA | Status |
|---|---|---|
| BgClean, BgMistake, BgBalance, BgEnhance | Backdrop cleanup, dust, background tone | partial |
| BgDeMoire | Fabric moiré (chroma filtering), by hand | partial |
| BgReplaceCustomColor | Solid background colour inside a mask | partial |
| BgShadow, BgShadowRetain | Drop shadow from a selected subject (ADR-0131); keeping the original shadow on a new background is not built | partial |
| BgFloorReflection, BgFloorReflectionPrepare | Floor reflection below a selected floor line (ADR-0131) | have |
| GlassReflectionRemoval | - | model |
| LensBlur | Background blur by mask; spin blur, privacy blur, soft focus (ADR-0131) | partial |
| LensCorrect, PerspectiveTransform | Lens and perspective correction (phase 23) | have |
| PostCropVignetting, RealGrain, Filters | Post-crop vignette, grain, presets and looks | have |
| Inpaint | Heal, clone, patch, cleanup; no generative fill | partial |
| MatteRefinement, MatteRefinementMix | Mask refinement, edge refine | have |
| CustomizeWaterMark | Export watermark | have |

## Workflow and colour (not effect modules)

| Evoto feature | AURA | Status |
|---|---|---|
| AI Color Match (reference image, Amount/Tone/Color, region masks, sync) | In progress in the working tree (`crates/aura-app/src/color_match.rs`) | partial |
| Batch sync, presets, per-person preset settings | Collection cleanup sync, presets, retouch strength sync | partial |
| Culling, export, delivery | Measured cull, export, delivery | have |
| Tethered shooting (camera SDKs, `Evoto-camera-link`) | - | build (needs vendor SDK licences) |

## Beyond Evoto's effect list (ADR-0131)

Catchlights (Evoto's "AI catchlights" page), freckles (its "freckles filter" page), glitch, light
leak and colour inversion (its creative-filter pages), plus tools a professional editor expects:
hue shift, green-screen colour replacement, local white balance, local sharpening, local noise
reduction, defringing, Orton glow, haze, sun rays, film halation, lip gloss and pixelation.
All are placed by hand; [Effects you place yourself](selected-effects.md) lists them.

## What is still to build without a model

1. **Makeup protection** - keep lip and eye makeup out of skin evening.
2. **Clothing edge smoothing, lip wrinkle repair, tan-line evening.**
3. **Keeping the original shadow** when a background colour is replaced.

The **model** rows - expression, eyelids, teeth repair, stubble removal, pose, background
generation - need a generative model. This build links no diffusion runtime, and a model without
a signed manifest and a card is refused (phase 03); none of them can be shipped by writing code.
