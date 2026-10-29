import { invoke } from './client';

/** One strength slider over a kind of automatic retouch (ADR-0130). */
export type RetouchCategory =
  | 'blemishes' | 'smoothing' | 'texture' | 'tone_evenness' | 'redness' | 'light_evenness'
  | 'wrinkles' | 'under_eye' | 'eyes' | 'teeth' | 'lips' | 'shine' | 'sculpt'
  | 'body_skin' | 'body_blemishes' | 'stretch_marks' | 'hair' | 'clothing' | 'background'
  | 'scene' | 'other';

export type IntensitySlider = {
  category: RetouchCategory;
  /** Automatic operations the slider scales on this photo. */
  operations: number;
  /** 1 is exactly what the automatic retouch decided. */
  factor: number;
  /** Above this every operation in the category is already at full strength. */
  headroom: number;
};

export type IntensityReport = { sliders: IntensitySlider[]; synced: number; skipped: number };

export const RETOUCH_CATEGORY_LABELS: Record<RetouchCategory, string> = {
  blemishes: 'Blemish & acne removal',
  smoothing: 'Skin smoothing',
  texture: 'Skin texture',
  tone_evenness: 'Even skin tone',
  redness: 'Redness',
  light_evenness: 'Even skin light',
  wrinkles: 'Wrinkles & lines',
  under_eye: 'Dark circles & eye bags',
  eyes: 'Eyes',
  teeth: 'Teeth whitening',
  lips: 'Lips & makeup',
  shine: 'Shine removal',
  sculpt: 'Face light & contour',
  body_skin: 'Body skin',
  body_blemishes: 'Body blemishes',
  stretch_marks: 'Stretch marks',
  hair: 'Hair',
  clothing: 'Clothing',
  background: 'Background cleanup',
  scene: 'Scene',
  other: 'Other automatic retouch',
};

const call = (projectId: string, photoId: string, action: 'get' | 'set' | 'reset' | 'sync', factors: Partial<Record<RetouchCategory, number>> = {}) =>
  invoke<IntensityReport>('retouch_intensity', { input: { projectId, photoId, action, factors } });

export const retouchIntensity = {
  get: (projectId: string, photoId: string) => call(projectId, photoId, 'get'),
  set: (projectId: string, photoId: string, factors: Partial<Record<RetouchCategory, number>>) => call(projectId, photoId, 'set', factors),
  reset: (projectId: string, photoId: string) => call(projectId, photoId, 'reset'),
  /** This photo's sliders onto every other automatically retouched photo in the collection. */
  sync: (projectId: string, photoId: string) => call(projectId, photoId, 'sync'),
};
