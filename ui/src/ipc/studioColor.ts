import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto } from './types';
export type LevelChannel = { inputBlack: number; inputWhite: number; gamma: number; outputBlack: number; outputWhite: number };
export type Levels = Record<'rgb' | 'red' | 'green' | 'blue', LevelChannel> & { luminosity: boolean };
export type Cmyk = { cyan: number; magenta: number; yellow: number; black: number };
export type GradeWheel = { hue: number; saturation: number; luminance: number };
export type LocalColor = { curve: { points: [number, number][] }; hsl: Record<string, { h: number; s: number; l: number }>; grade: Record<'shadows' | 'midtones' | 'highlights' | 'global', GradeWheel> & { blending: number; balance: number }; sharpen: number; dehaze: number; vibrance: number };
export type StudioColor = { levels: Levels; selective: { absolute: boolean; ranges: Record<string, Cmyk> }; local: Record<string, LocalColor> };
export const LEVEL_NEUTRAL: LevelChannel = { inputBlack: 0, inputWhite: 1, gamma: 1, outputBlack: 0, outputWhite: 1 };
export const CMYK_NEUTRAL: Cmyk = { cyan: 0, magenta: 0, yellow: 0, black: 0 };
export const SELECTIVE_RANGES = ['reds', 'yellows', 'greens', 'cyans', 'blues', 'magentas', 'whites', 'neutrals', 'blacks'] as const;
export const HSL_BANDS = ['red', 'orange', 'yellow', 'green', 'aqua', 'blue', 'purple', 'magenta'] as const;
export function neutralLocalColor(): LocalColor {
  return { curve: { points: [[0, 0], [255, 255]] }, hsl: {}, grade: { shadows: { hue: 0, saturation: 0, luminance: 0 }, midtones: { hue: 0, saturation: 0, luminance: 0 }, highlights: { hue: 0, saturation: 0, luminance: 0 }, global: { hue: 0, saturation: 0, luminance: 0 }, blending: 50, balance: 0 }, sharpen: 0, dehaze: 0, vibrance: 0 };
}
export function colorOf(recipe: RecipeDto | null): StudioColor {
  const neutral: StudioColor = { levels: { rgb: { ...LEVEL_NEUTRAL }, red: { ...LEVEL_NEUTRAL }, green: { ...LEVEL_NEUTRAL }, blue: { ...LEVEL_NEUTRAL }, luminosity: false }, selective: { absolute: false, ranges: {} }, local: {} };
  try { const parsed = JSON.parse(recipe?.body ?? '{}') as { studio_color_v1?: StudioColor }; return parsed.studio_color_v1 ?? neutral; } catch { return neutral; }
}
export const studioColor = {
  save: (projectId: string, photoId: string, recipeHash: string, color: StudioColor) => invoke<{ color: StudioColor; recipe: RecipeDto }>('save_studio_color', { input: { projectId, photoId, recipeHash, color } }),
  measure: (projectId: string, photoId: string, recipeHash: string, kind: 'auto' | 'black' | 'white' | 'hsl', channel: 'rgb' | 'red' | 'green' | 'blue', x: number, y: number) => invoke<{ color: StudioColor; recipeHash: string; description: string; hslBand?: string | null }>('measure_studio_color', { input: { projectId, photoId, recipeHash, kind, channel, x, y } }),
};
