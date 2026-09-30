import { invoke } from '@tauri-apps/api/core';
import type { RenderDto } from './types';

export const RETOUCH_TOOLS = [
  ['heal', 'Heal blemish / flyaway / lint', 'Repair', 'Samples nearby pixels; choose a source for precise repairs.'],
  ['clone', 'Clone stamp', 'Repair', 'Copies the selected source patch into the target.'],
  ['auto_blemish', 'Auto spot cleanup', 'Skin', 'Finds small dark spots inside your selection. Review permanent marks and fine details afterward.'],
  ['frequency', 'Frequency separation', 'Skin', 'Adjust tonal unevenness and fine texture independently.'],
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
export type NativeRetouchEdit = {
  id: string; tool: RetouchTool; enabled: boolean; region: [number, number, number, number];
  source: [number, number] | null; amount: number; feather: number; radius: number;
  texture: number; tone: number; warmth: number; tint: number;
};
export const freshRetouch = (): NativeRetouchEdit => ({id:'draft',tool:'heal',enabled:true,region:[0.5,0.45,0.035,0.035],source:null,amount:0.65,feather:0.65,radius:0.003,texture:1,tone:0.5,warmth:0,tint:0});
export const nativeRetouch = {
  edit: (projectId: string, photoId: string, action: 'list'|'append'|'update'|'remove'|'clear', edits: NativeRetouchEdit[] = [], id: string|null = null) => invoke<NativeRetouchEdit[]>('native_retouch_edit',{input:{projectId,photoId,action,edits,id}}),
  preview: (projectId: string, photoId: string, before = false) => invoke<RenderDto>('native_retouch_preview',{projectId,photoId,before}),
};
