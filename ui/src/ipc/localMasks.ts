import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto, RenderDto } from './types';
import type { BrushStroke } from './nativeRetouch';

/** Masks in the Studio (ADR-0102): each is a list of components combined in order. */
export type MaskMode = 'add' | 'subtract' | 'intersect';
export type LuminanceRange = { low: number; high: number; softness: number };
export type MaskSource =
  | { type: 'matte'; matte: string; what: string }
  | { type: 'region'; region: string }
  | { type: 'linear'; start: [number, number]; end: [number, number] }
  | { type: 'radial'; centre: [number, number]; radii: [number, number]; angle: number; feather: number }
  | { type: 'brush'; strokes: BrushStroke[]; feather: number }
  | { type: 'luminance'; range: LuminanceRange };
export type MaskComponent = { mode: MaskMode; invert?: boolean; source: MaskSource };
/** The sliders inside a mask. Absent means "not touched here". */
export type MaskParams = Partial<Record<'contrast' | 'highlights' | 'shadows' | 'whites' | 'blacks' | 'clarity' | 'texture' | 'saturation' | 'tint', number>>
  & { exposure?: number; temperature?: number };
export type LocalMask = { id: string; name: string; enabled: boolean; amount: number; components: MaskComponent[]; params: MaskParams };
export type LocalMasksDto = { masks: LocalMask[]; recipe: RecipeDto; maskId: string | null; message: string | null };
export type MaskCoverage = Pick<RenderDto, 'width' | 'height' | 'rgbBase64'>;

/** The selections the backend measures from the photograph, by name. */
export const AI_SELECTIONS: ReadonlyArray<readonly [string, string, string]> = [
  ['subject', 'Subject', 'The people in the photograph'],
  ['background', 'Background', 'Everything except the people'],
  ['sky', 'Sky', 'Open sky above a horizon'],
  ['face_skin', 'Face skin', 'People'],
  ['body_skin', 'Body skin', 'People'],
  ['hair', 'Hair', 'People'],
  ['clothes', 'Clothes', 'People'],
  ['eyes', 'Eyes', 'Face'],
  ['iris', 'Irises', 'Face'],
  ['sclera', 'Whites of the eyes', 'Face'],
  ['eyebrows', 'Eyebrows', 'Face'],
  ['lips', 'Lips', 'Face'],
  ['teeth', 'Teeth', 'Face'],
  ['facial_hair', 'Beard and moustache', 'Face'],
] as const;

/** Slider definitions: path, label, minimum, maximum, step. */
export const MASK_SLIDERS: ReadonlyArray<readonly [keyof MaskParams, string, number, number, number]> = [
  ['exposure', 'Exposure', -4, 4, 0.05],
  ['contrast', 'Contrast', -100, 100, 1],
  ['highlights', 'Highlights', -100, 100, 1],
  ['shadows', 'Shadows', -100, 100, 1],
  ['whites', 'Whites', -100, 100, 1],
  ['blacks', 'Blacks', -100, 100, 1],
  ['temperature', 'Temperature', -2000, 2000, 50],
  ['tint', 'Tint', -100, 100, 1],
  ['saturation', 'Saturation', -100, 100, 1],
  ['clarity', 'Clarity', -100, 100, 1],
  ['texture', 'Texture', -100, 100, 1],
] as const;

export type CreateMask = { what: string; source?: MaskSource | null; into?: string | null; mode?: MaskMode | null; invert?: boolean };

export const localMasks = {
  list: (projectId: string, photoId: string) => invoke<LocalMasksDto>('local_masks', { input: { projectId, photoId } }),
  create: (projectId: string, photoId: string, request: CreateMask) =>
    invoke<LocalMasksDto>('create_local_mask', { input: { projectId, photoId, what: request.what, source: request.source ?? null, into: request.into ?? null, mode: request.mode ?? null, invert: request.invert ?? false } }),
  save: (projectId: string, photoId: string, masks: LocalMask[], label: string | null = null) =>
    invoke<LocalMasksDto>('save_local_masks', { input: { projectId, photoId, masks, label } }),
  coverage: (projectId: string, photoId: string, maskId: string) =>
    invoke<MaskCoverage>('local_mask_coverage', { input: { projectId, photoId, maskId } }),
};

/** Read the masks out of a recipe body without a round trip. */
export function masksOf(recipe: RecipeDto | null): LocalMask[] {
  if (!recipe) return [];
  try {
    const body = JSON.parse(recipe.body) as Record<string, unknown>;
    const masks = body['studio_masks_v1'];
    return Array.isArray(masks) ? masks as LocalMask[] : [];
  } catch { return []; }
}

/** A short description of a component for the mask list. */
export function describe(component: MaskComponent): string {
  const verb = component.mode === 'add' ? '+' : component.mode === 'subtract' ? '−' : '∩';
  const source = component.source;
  const name = source.type === 'matte' ? AI_SELECTIONS.find(([id]) => id === source.what)?.[1] ?? source.what
    : source.type === 'region' ? AI_SELECTIONS.find(([id]) => id === source.region)?.[1] ?? source.region
      : source.type === 'linear' ? 'Linear gradient' : source.type === 'radial' ? 'Radial gradient'
        : source.type === 'brush' ? 'Brush' : 'Brightness range';
  return `${verb} ${component.invert ? 'not ' : ''}${name}`;
}
