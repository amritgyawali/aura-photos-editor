import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto, RenderDto } from './types';

export const RETOUCH_TOOLS = [
  ['acne_clear', 'Acne & blemish clear · brush', 'Repair', 'Paint over pimples, red or brown marks and bumps that are left. Each one is measured against the clean skin around it and rebuilt from that skin, keeping the pores. Works on the nose and between the brows too; creases, hair and nostrils are left alone.'],
  ['heal', 'Heal blemish / flyaway / lint', 'Repair', 'Samples nearby pixels; choose a source for precise repairs.'],
  ['patch_heal', 'Texture-aware patch heal', 'Repair', 'Matches nearby texture and blends surrounding light. Auto source works on small ellipses; choose a source for painted or larger repairs.'],
  ['frequency_heal', 'Frequency healing · marks', 'Repair', 'Finds compact marks in the selection and rebuilds the tone under each one from the clean skin around it. Pores stay where they are; creases and hair are left alone.'],
  ['clone', 'Clone stamp', 'Repair', 'Copies the selected source patch into the target.'],
  ['auto_blemish', 'Auto spot cleanup', 'Skin', 'Finds small dark spots inside your selection. Review permanent marks and fine details afterward.'],
  ['frequency', 'Frequency separation', 'Skin', 'Adjust tonal unevenness and fine texture independently.'],
  ['texture_graft', 'Restore skin texture', 'Skin', 'Puts this skin’s own pore detail back where earlier steps removed it, in the same place, with glints and deep pits limited; healed blemishes borrow pores from clean skin nearby. Nothing is generated.'],
  ['skin_smooth', 'Skin smoothing · protect detail', 'Sampled skin', 'Smooths uneven texture between fine detail and facial form. Sample skin first; similar colors inside your selection receive the effect.'],
  ['skin_uniformity', 'Even sampled skin tone', 'Sampled skin', 'Reduces color differences toward your skin sample while preserving brightness. Select one person at a time.'],
  ['portrait_dodge_burn', 'Skin dodge and burn · protect edges', 'Sampled skin', 'Balances local light with bounded exposure changes while preserving RGB proportions. Sample skin and review facial edges.'],
  ['micro_dodge_burn', 'Micro dodge and burn', 'Light', 'Evens small luminance variations while retaining color.'],
  ['dodge', 'Dodge / highlight sculpting', 'Light', 'Lightens the selected region with a feathered mask.'],
  ['burn', 'Burn / shadow sculpting', 'Light', 'Darkens the selected region with a feathered mask.'],
  ['skin_color', 'Skin color correction', 'Color', 'Changes warmth and tint while retaining luminance.'],
  ['color_match', 'Match complexion to sample', 'Color', 'Matches target chroma to a source region without copying its texture.'],
  ['mattify', 'Reduce oily shine', 'Skin', 'Attenuates local highlights without replacing skin texture.'],
  ['under_eye', 'Under-eye shadow lift', 'Skin', 'Lifts local shadows; select only the under-eye area.'],
  ['wrinkle', 'Soften fine lines', 'Skin', 'Reduces mid-frequency contrast; texture remains separately adjustable.'],
  ['teeth', 'Natural teeth whitening', 'Details', 'Reduces yellow casts and gently lifts tooth brightness.'],
  ['eye_clean', 'Eye redness reduction', 'Details', 'Reduces excess red chroma inside the selected sclera.'],
  ['eye_detail', 'Iris and eyelash detail', 'Details', 'Adds fine local contrast to existing eye detail.'],
  ['red_eye', 'Flash red-eye correction', 'Details', 'Reduces dominant red inside the selected pupil.'],
  ['fabric', 'Fabric crease softening', 'Cleanup', 'Reduces broad texture variations; keep seams outside the selection.'],
  ['backdrop', 'Backdrop smoothing', 'Cleanup', 'Smooths a selected backdrop patch. Avoid subject edges.'],
  ['glare', 'Glare softening', 'Details', 'Reduces bright reflections; cannot reconstruct detail hidden by glare.'],
  ['makeup', 'Local cosmetic tint', 'Color', 'Applies a controlled warm or magenta tint to a chosen region.'],
] as const;
export type RetouchTool = typeof RETOUCH_TOOLS[number][0];
export type BrushPoint = [number, number, number];
export type BrushStroke = { erase: boolean; radius: number; opacity: number; points: BrushPoint[] };
export type BrushMask = { strokes: BrushStroke[] };
/** `connected` limits the change to skin touching the sample, never a same-coloured background. */
export type SkinSettings = { tolerance: number; edgeProtection: number; connected?: boolean };
export type RetouchSelection = {
  inverted?: boolean;
  gradient?: { start: [number, number]; end: [number, number] } | null;
  luminance?: { low: number; high: number; softness: number } | null;
};
export type SelectionPreview = Pick<RenderDto, 'width'|'height'|'rgbBase64'>;
/** Which automatic finishing runs and how strongly; remembered by the next Auto enhance. */
/** Which skin the automatic retouch may change. */
export type RetouchScope = 'face' | 'body' | 'face_and_body';
/**
 * The automatic retouch's fine controls (ADR-0082), mirroring `retouch_settings::Settings`.
 * Strengths are 0..1 and 0 switches an operation off; for measured corrections 0.5 is the
 * measured strength. Signed values are -1..1. Missing keys take the Rust defaults.
 */
export type RetouchSettings = {
  aiSkinDetection: boolean; mainSubjectOnly: boolean; maskPrecision: number; edgeSoftness: number; protectFacialHair: boolean; protectEyeArea: boolean; protectNoseDetail: boolean;
  smoothing: number; texture: number; smoothingSize: number; toneEvenness: number; lightEvenness: number; microDodgeBurn: number;
  poreRefine: number; shine: number; redness: number; glow: number; skinBrightness: number; skinWarmth: number; skinTint: number;
  blemishSensitivity: number; maxSpots: number; keepFreckles: boolean;
  deepBlemishCleanup: boolean; removeDarkMarks: boolean;
  /** Frequency healing and the texture graft (ADR-0090); 0 is off. */
  frequencyHeal: number; textureGraft: number;
  foreheadLines: number; crowsFeet: number; smileLines: number; underEyeLines: number; neckLines: number;
  darkCircles: number; eyeBags: number;
  eyeWhitening: number; eyeVessels: number; irisDetail: number; irisBrightness: number; redEye: boolean; lashDefinition: number; browDefinition: number;
  teethWhitening: number; lipColour: number; lipDefinition: number;
  contour: number; highlight: number; blush: number; faceLight: number;
  bodySmoothing: number; bodyTone: number; matchBodyToFace: number; bodyShine: number; bodyRedness: number; bodyBlemishes: number;
  hairDetail: number; hairShine: number; fabric: number; backdrop: number;
};
export const DEFAULT_RETOUCH_SETTINGS: RetouchSettings = {
  aiSkinDetection: true, mainSubjectOnly: false, maskPrecision: .5, edgeSoftness: .35, protectFacialHair: true, protectEyeArea: true, protectNoseDetail: true,
  smoothing: .5, texture: .85, smoothingSize: .5, toneEvenness: .5, lightEvenness: .5, microDodgeBurn: .25,
  poreRefine: 0, shine: .5, redness: .5, glow: 0, skinBrightness: 0, skinWarmth: 0, skinTint: 0,
  blemishSensitivity: .5, maxSpots: 12, keepFreckles: true,
  deepBlemishCleanup: false, removeDarkMarks: false,
  frequencyHeal: 0, textureGraft: 0,
  foreheadLines: .5, crowsFeet: .5, smileLines: .5, underEyeLines: .25, neckLines: 0,
  darkCircles: .5, eyeBags: .25,
  eyeWhitening: .2, eyeVessels: .5, irisDetail: .5, irisBrightness: 0, redEye: true, lashDefinition: 0, browDefinition: 0,
  teethWhitening: .5, lipColour: 0, lipDefinition: 0,
  contour: 0, highlight: 0, blush: 0, faceLight: 0,
  bodySmoothing: .5, bodyTone: .5, matchBodyToFace: 0, bodyShine: .25, bodyRedness: 0, bodyBlemishes: 0,
  hairDetail: 0, hairShine: 0, fabric: 0, backdrop: 0,
};
/** `adaptive` measures each face and tunes the fine controls for it (ADR-0086); missing means on. */
export type AutoRetouchOptions = { intensity: number; blemishes: boolean; eyes: boolean; teeth: boolean; refine: boolean; scope: RetouchScope; settings?: RetouchSettings; adaptive?: boolean };
export const DEFAULT_AUTO_RETOUCH: AutoRetouchOptions = { intensity: 1, blemishes: true, eyes: true, teeth: true, refine: true, scope: 'face', adaptive: true };
export const DEFAULT_SKIN: SkinSettings = { tolerance: .08, edgeProtection: .8, connected: true };
export const isSampledSkinTool = (tool: RetouchTool) => ['skin_smooth', 'skin_uniformity', 'portrait_dodge_burn'].includes(tool);
export type NativeRetouchEdit = {
  id: string; tool: RetouchTool; enabled: boolean; region: [number, number, number, number];
  source: [number, number] | null; amount: number; feather: number; radius: number;
  sourceScale?: number;
  preserveMicrotexture?: boolean;
  textureHeal?: boolean;
  /** Frequency healing and acne clear: how readily a deviation counts as a mark (default 0.5). */
  sensitivity?: number | null;
  /** Frequency healing and acne clear: leave marks that are darker but not redder or browner. */
  keepDarkMarks?: boolean;
  texture: number; tone: number; warmth: number; tint: number;
  mask?: BrushMask | null;
  skin?: SkinSettings | null;
  selection?: RetouchSelection | null;
  /** A segmentation matte stored with the recipe (face skin, body skin, hair, clothes). */
  matte?: string | null;
};
export const extendedRetouchShape = (edit: NativeRetouchEdit) => Boolean(edit.selection?.inverted || edit.selection?.gradient);
export const needsRetouchSource = (edit: NativeRetouchEdit) => ['clone', 'color_match'].includes(edit.tool)
  || isSampledSkinTool(edit.tool)
  || (edit.tool === 'heal' && extendedRetouchShape(edit))
  || (edit.tool === 'patch_heal' && (Boolean(edit.mask) || edit.region[2] > .1 || edit.region[3] > .1 || extendedRetouchShape(edit)));
export function validRetouchSelection(edit: NativeRetouchEdit): boolean {
  const { gradient, luminance } = edit.selection ?? {};
  if (gradient && (edit.mask || ![...gradient.start,...gradient.end].every(v => Number.isFinite(v) && v >= 0 && v <= 1)
    || Math.hypot(gradient.start[0]-gradient.end[0],gradient.start[1]-gradient.end[1]) < .001)) return false;
  return !luminance || ([luminance.low,luminance.high,luminance.softness].every(Number.isFinite)
    && luminance.low >= -16 && luminance.high <= 16 && luminance.low <= luminance.high && luminance.softness >= 0 && luminance.softness <= 4);
}
/** The blemish brush: acne clear limited to what a person paints. Paint over what is left after
 * the automatic retouch; the marks inside are rebuilt from the clean skin around them. */
export const blemishBrush = (region: NativeRetouchEdit['region']): NativeRetouchEdit => ({
  id: 'draft', tool: 'acne_clear', enabled: true, region, source: null, amount: 1, feather: .35,
  radius: .005, texture: .25, tone: 1, warmth: 0, tint: 0, sensitivity: .75, keepDarkMarks: false,
  preserveMicrotexture: true, mask: { strokes: [] }, matte: null, selection: null, skin: null,
});
export const freshRetouch = (): NativeRetouchEdit => ({id:'draft',tool:'heal',enabled:true,region:[0.5,0.45,0.035,0.035],source:null,amount:0.65,feather:0.65,radius:0.003,texture:1,tone:0.5,warmth:0,tint:0});
/** How sharp a preview is: the original's own resolution, or a screen-sized first look. */
export type PreviewQuality = 'full' | 'fast';
export const nativeRetouch = {
  autoPortrait: (photoId: string) => invoke<RecipeDto>('enhance_portrait', { input: { photoId } }),
  autoRetouch: (projectId: string, photoId: string, options: AutoRetouchOptions, global = false) => invoke<RecipeDto>('auto_retouch', { input: { projectId, photoId, global, options } }),
  edit: (projectId: string, photoId: string, action: 'list'|'append'|'update'|'remove'|'clear'|'duplicate'|'earlier'|'later', edits: NativeRetouchEdit[] = [], id: string|null = null) => invoke<NativeRetouchEdit[]>('native_retouch_edit',{input:{projectId,photoId,action,edits,id}}),
  /** The retouch view's photograph: `full` is the original's own resolution (ADR-0097), `fast` the first look. */
  preview: (projectId: string, photoId: string, before = false, quality: PreviewQuality = 'full') => invoke<RenderDto>('native_retouch_preview',{projectId,photoId,before,quality}),
  /** The photograph as taken, with no edits: what Original and Compare show. */
  original: (projectId: string, photoId: string, quality: PreviewQuality = 'full') => invoke<RenderDto>('photo_original',{projectId,photoId,quality}),
  draftPreview: (projectId: string, photoId: string, edit: NativeRetouchEdit, replaceId: string|null, quality: PreviewQuality = 'full') => invoke<RenderDto>('native_retouch_draft_preview',{input:{projectId,photoId,edit,replaceId,quality}}),
  selectionPreview: (projectId: string, photoId: string, edit: NativeRetouchEdit, replaceId: string|null) => invoke<SelectionPreview>('native_retouch_selection_preview',{input:{projectId,photoId,edit,replaceId}}),
  savedSelection: (projectId: string, photoId: string, operationId: string|null) => invoke<SelectionPreview>('native_retouch_saved_selection',{input:{projectId,photoId,operationId}}),
};
