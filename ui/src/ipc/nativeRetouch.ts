import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto, RenderDto } from './types';

export const RETOUCH_TOOLS = [
  ['heal', 'Heal blemish / flyaway / lint', 'Repair', 'Samples nearby pixels; choose a source for precise repairs.'],
  ['patch_heal', 'Texture-aware patch heal', 'Repair', 'Matches nearby texture and blends surrounding light. Auto source works on small ellipses; choose a source for painted or larger repairs.'],
  ['clone', 'Clone stamp', 'Repair', 'Copies the selected source patch into the target.'],
  ['auto_blemish', 'Auto spot cleanup', 'Skin', 'Finds small dark spots inside your selection. Review permanent marks and fine details afterward.'],
  ['frequency', 'Frequency separation', 'Skin', 'Adjust tonal unevenness and fine texture independently.'],
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
export type SkinSettings = { tolerance: number; edgeProtection: number };
export type RetouchSelection = {
  inverted?: boolean;
  gradient?: { start: [number, number]; end: [number, number] } | null;
  luminance?: { low: number; high: number; softness: number } | null;
};
export type SelectionPreview = Pick<RenderDto, 'width'|'height'|'rgbBase64'>;
/** Which automatic finishing runs and how strongly; remembered by the next Auto enhance. */
/** Which skin the automatic retouch may change. */
export type RetouchScope = 'face' | 'body' | 'face_and_body';
export type AutoRetouchOptions = { intensity: number; blemishes: boolean; eyes: boolean; teeth: boolean; refine: boolean; scope: RetouchScope };
export const DEFAULT_AUTO_RETOUCH: AutoRetouchOptions = { intensity: 1, blemishes: true, eyes: true, teeth: true, refine: true, scope: 'face' };
export const DEFAULT_SKIN: SkinSettings = { tolerance: .08, edgeProtection: .8 };
export const isSampledSkinTool = (tool: RetouchTool) => ['skin_smooth', 'skin_uniformity', 'portrait_dodge_burn'].includes(tool);
export type NativeRetouchEdit = {
  id: string; tool: RetouchTool; enabled: boolean; region: [number, number, number, number];
  source: [number, number] | null; amount: number; feather: number; radius: number;
  texture: number; tone: number; warmth: number; tint: number;
  mask?: BrushMask | null;
  skin?: SkinSettings | null;
  selection?: RetouchSelection | null;
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
export const freshRetouch = (): NativeRetouchEdit => ({id:'draft',tool:'heal',enabled:true,region:[0.5,0.45,0.035,0.035],source:null,amount:0.65,feather:0.65,radius:0.003,texture:1,tone:0.5,warmth:0,tint:0});
export const nativeRetouch = {
  autoPortrait: (photoId: string) => invoke<RecipeDto>('enhance_portrait', { input: { photoId } }),
  autoRetouch: (projectId: string, photoId: string, options: AutoRetouchOptions, global = false) => invoke<RecipeDto>('auto_retouch', { input: { projectId, photoId, global, options } }),
  edit: (projectId: string, photoId: string, action: 'list'|'append'|'update'|'remove'|'clear'|'duplicate'|'earlier'|'later', edits: NativeRetouchEdit[] = [], id: string|null = null) => invoke<NativeRetouchEdit[]>('native_retouch_edit',{input:{projectId,photoId,action,edits,id}}),
  preview: (projectId: string, photoId: string, before = false) => invoke<RenderDto>('native_retouch_preview',{projectId,photoId,before}),
  draftPreview: (projectId: string, photoId: string, edit: NativeRetouchEdit, replaceId: string|null) => invoke<RenderDto>('native_retouch_draft_preview',{input:{projectId,photoId,edit,replaceId}}),
  selectionPreview: (projectId: string, photoId: string, edit: NativeRetouchEdit, replaceId: string|null) => invoke<SelectionPreview>('native_retouch_selection_preview',{input:{projectId,photoId,edit,replaceId}}),
};
