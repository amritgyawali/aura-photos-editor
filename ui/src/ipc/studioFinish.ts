import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto } from './types';

/**
 * The Studio's finishing tools (ADR-0108): face and body shape, liquify, background replacement,
 * and feature colour and makeup - what a portrait editor such as Evoto offers beside skin work.
 * Every slider is -100..100 and zero means untouched.
 */
export type FaceShape = {
  slim: number; jaw: number; chin: number; forehead: number; cheekbones: number;
  eyeSize: number; eyeDistance: number; noseWidth: number; noseLength: number;
  mouthWidth: number; lips: number; smile: number; headSize: number;
};
export type BodyShape = { slim: number; waist: number; hips: number; arms: number; shoulders: number; legs: number; neck: number };
export type LiquifyMode = 'push' | 'bloat' | 'pinch' | 'restore';
export type LiquifyStroke = { mode: LiquifyMode; radius: number; strength: number; points: [number, number][] };
export type BackgroundMode = 'colour' | 'gradient' | 'blur' | 'sky';
/** Colours are sRGB in 0..1, as a colour picker shows them. */
export type Background = { mode: BackgroundMode; colour: [number, number, number]; colour2: [number, number, number]; amount: number; feather: number };
export type Tint = { colour: [number, number, number]; amount: number };
export type FeatureColours = { hair: Tint | null; eyes: Tint | null; lips: Tint | null; blush: Tint | null; eyeshadow: Tint | null; eyebrows: Tint | null };
export type StudioFinish = { face: FaceShape; body: BodyShape; liquify?: LiquifyStroke[]; background?: Background | null; colours: FeatureColours };
export type StudioFinishDto = { finish: StudioFinish; recipe: RecipeDto };

export const FACE_SLIDERS: ReadonlyArray<readonly [keyof FaceShape, string]> = [
  ['slim', 'Face slim'], ['jaw', 'Jawline'], ['chin', 'Chin length'], ['forehead', 'Forehead height'],
  ['cheekbones', 'Cheekbones'], ['eyeSize', 'Eye size'], ['eyeDistance', 'Eye distance'],
  ['noseWidth', 'Nose slim'], ['noseLength', 'Nose length'], ['mouthWidth', 'Mouth width'],
  ['lips', 'Lip fullness'], ['smile', 'Smile'], ['headSize', 'Head size'],
] as const;

export const BODY_SLIDERS: ReadonlyArray<readonly [keyof BodyShape, string]> = [
  ['slim', 'Body slim'], ['waist', 'Waist'], ['hips', 'Hips'], ['arms', 'Arms'],
  ['shoulders', 'Shoulders'], ['legs', 'Leg length'], ['neck', 'Neck length'],
] as const;

export const COLOUR_FEATURES: ReadonlyArray<readonly [keyof FeatureColours, string, string]> = [
  ['hair', 'Hair colour', '#7a3b1e'], ['eyes', 'Eye colour', '#3f6f9f'], ['lips', 'Lipstick', '#b0303f'],
  ['blush', 'Blush', '#e48a8a'], ['eyeshadow', 'Eyeshadow', '#8a5a7a'], ['eyebrows', 'Brow colour', '#3a2a20'],
] as const;

export const NEUTRAL_FACE: FaceShape = { slim: 0, jaw: 0, chin: 0, forehead: 0, cheekbones: 0, eyeSize: 0, eyeDistance: 0, noseWidth: 0, noseLength: 0, mouthWidth: 0, lips: 0, smile: 0, headSize: 0 };
export const NEUTRAL_BODY: BodyShape = { slim: 0, waist: 0, hips: 0, arms: 0, shoulders: 0, legs: 0, neck: 0 };
export const NEUTRAL_FINISH: StudioFinish = {
  face: NEUTRAL_FACE, body: NEUTRAL_BODY, liquify: [], background: null,
  colours: { hair: null, eyes: null, lips: null, blush: null, eyeshadow: null, eyebrows: null },
};

/** `#rrggbb` to sRGB 0..1. */
export function hexToRgb(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  const n = m?.[1] ? parseInt(m[1], 16) : 0xffffff;
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** sRGB 0..1 to `#rrggbb`. */
export function rgbToHex(rgb: readonly number[]): string {
  return `#${rgb.slice(0, 3).map(v => Math.round(Math.min(1, Math.max(0, v ?? 0)) * 255).toString(16).padStart(2, '0')).join('')}`;
}

/** A finish as the backend may have stored it, with every absent block filled with zeroes. */
export function normaliseFinish(value: Partial<StudioFinish> | null | undefined): StudioFinish {
  return {
    face: { ...NEUTRAL_FACE, ...(value?.face ?? {}) },
    body: { ...NEUTRAL_BODY, ...(value?.body ?? {}) },
    liquify: value?.liquify ?? [],
    background: value?.background ?? null,
    colours: { ...NEUTRAL_FINISH.colours, ...(value?.colours ?? {}) },
  };
}

export const studioFinish = {
  get: (projectId: string, photoId: string) =>
    invoke<StudioFinishDto>('studio_finish', { input: { projectId, photoId } }),
  save: (projectId: string, photoId: string, finish: StudioFinish, label: string) =>
    invoke<StudioFinishDto>('save_studio_finish', { input: { projectId, photoId, finish, label } }),
};

/** The finish a recipe carries, read from its body; neutral when absent or unreadable. */
export function finishOf(recipe: RecipeDto | null): StudioFinish {
  if (!recipe) return normaliseFinish(null);
  try {
    const body = JSON.parse(recipe.body) as Record<string, unknown>;
    const value = body['studio_finish_v1'];
    return normaliseFinish(value && typeof value === 'object' ? value as Partial<StudioFinish> : null);
  } catch { return normaliseFinish(null); }
}

/** One-click looks, the way a portrait editor's presets start an edit. Each is a starting point. */
export const FINISH_PRESETS: ReadonlyArray<{ id: string; name: string; hint: string; apply: (f: StudioFinish) => StudioFinish }> = [
  { id: 'natural-face', name: 'Natural face refine', hint: 'Slightly slimmer face and jaw, a touch larger eyes.',
    apply: f => ({ ...f, face: { ...f.face, slim: 15, jaw: 12, eyeSize: 10, noseWidth: 8 } }) },
  { id: 'body-tone', name: 'Body contour', hint: 'Gentle waist and arm slimming, longer legs.',
    apply: f => ({ ...f, body: { ...f.body, waist: 20, arms: 15, legs: 15 } }) },
  { id: 'white-backdrop', name: 'Clean white backdrop', hint: 'Pure white behind the person - headshots, ID and e-commerce.',
    apply: f => ({ ...f, background: { mode: 'colour', colour: [1, 1, 1], colour2: [1, 1, 1], amount: 100, feather: 25 } }) },
  { id: 'studio-grey', name: 'Studio grey gradient', hint: 'A classic portrait backdrop, light to dark.',
    apply: f => ({ ...f, background: { mode: 'gradient', colour: [0.62, 0.63, 0.65], colour2: [0.22, 0.23, 0.25], amount: 100, feather: 30 } }) },
  { id: 'bokeh', name: 'Background blur', hint: 'Blur everything behind the people, like a wide aperture.',
    apply: f => ({ ...f, background: { mode: 'blur', colour: [1, 1, 1], colour2: [1, 1, 1], amount: 60, feather: 30 } }) },
  { id: 'blue-sky', name: 'Blue sky', hint: 'Replace a blown or grey sky with a clear blue one.',
    apply: f => ({ ...f, background: { mode: 'sky', colour: [0.26, 0.5, 0.86], colour2: [0.72, 0.84, 0.96], amount: 100, feather: 40 } }) },
  { id: 'sunset-sky', name: 'Sunset sky', hint: 'A warm golden-hour sky.',
    apply: f => ({ ...f, background: { mode: 'sky', colour: [0.36, 0.33, 0.6], colour2: [0.98, 0.66, 0.4], amount: 100, feather: 40 } }) },
];
