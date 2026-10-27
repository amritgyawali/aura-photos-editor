import { invoke } from '@tauri-apps/api/core';
import type { RecipeDto } from './types';

/**
 * Evoto-style framing, delivery and review (ADR-0150 group J): the faces in a photograph for the
 * face picker, ID photographs and measured crops, cut-outs and social exports written into a
 * folder the photographer chose, and a blink check across a collection.
 */
export type EvotoFace = {
  /** `[left, top, right, bottom]`, normalised. */
  bounds: [number, number, number, number];
  /** Normalised centre: what the finish stores to choose this face. */
  centre: [number, number];
  /** Eye-line tilt in degrees, clockwise. */
  tilt: number;
  /** How open the image-left and image-right eyes are; an open eye measures about 0.08. */
  eyes: [number, number] | null;
  closed: boolean;
  confidence: number;
};

export type FramingPreset = 'id_35x45' | 'id_2x2' | 'id_33x48' | 'headshot' | 'half_body' | 'full_body' | 'square' | 'story' | 'level_eyes';
export type FramingResult = { crop: [number, number, number, number]; rotate: number; whiteBackground: boolean; reasons: string[]; recipe: RecipeDto };

export type ExportKind = 'png_cutout' | 'grid' | 'carousel' | 'before_after' | 'upscale_2x';
export type ExportResult = { files: string[]; width: number; height: number; reasons: string[] };

export type BlinkPhoto = { photoId: string; fileName: string; faces: EvotoFace[]; closed: number; unchecked: number };
export type BlinkReport = { checked: number; flagged: BlinkPhoto[]; withoutFaces: number };

export const FRAMING_PRESETS: ReadonlyArray<readonly [FramingPreset, string, string]> = [
  ['id_35x45', 'Passport 35×45 mm', 'Nepal, India, UK, Schengen and most passports: head 74% of the height, levelled, on white.'],
  ['id_2x2', 'US 2×2 in', 'US passport and visa: head 60% of the height, levelled, on white.'],
  ['id_33x48', 'Visa 33×48 mm', 'China visa: head 64% of the height, levelled, on white.'],
  ['headshot', 'Headshot 4:5', 'Head and shoulders, eyes on the upper third.'],
  ['half_body', 'Half body 4:5', 'From above the head to the waist.'],
  ['full_body', 'Full body 2:3', 'The whole person with a little room above and below.'],
  ['square', 'Square face', 'A 1:1 crop around the face, for profile pictures.'],
  ['story', 'Story 9:16', 'A full-height vertical crop centred on the face.'],
  ['level_eyes', 'Level the eyes', 'Straightens the photograph so the eye line is level, without empty corners.'],
] as const;

export const EXPORT_KINDS: ReadonlyArray<readonly [ExportKind, string, string]> = [
  ['png_cutout', 'Transparent PNG cut-out', 'The people on a transparent background.'],
  ['grid', 'Instagram grid', 'Three squares a row, numbered in the order to post them.'],
  ['carousel', 'Carousel slides', 'A wide photograph split into 4:5 slides that swipe seamlessly.'],
  ['before_after', 'Before and after', 'The original and the edit side by side, in the same framing.'],
  ['upscale_2x', 'Enlarge 2×', 'Twice the size with a Lanczos filter that does not ring at edges.'],
] as const;

export const evotoWorkflow = {
  faces: (projectId: string, photoId: string) =>
    invoke<{ faces: EvotoFace[] }>('evoto_faces', { input: { projectId, photoId } }),
  frame: (projectId: string, photoId: string, preset: FramingPreset, face?: [number, number] | null, level = false) =>
    invoke<FramingResult>('apply_evoto_framing', { input: { projectId, photoId, preset, face: face ?? null, level } }),
  export: (projectId: string, photoId: string, kind: ExportKind, folder: string, count?: number) =>
    invoke<ExportResult>('evoto_export', { input: { projectId, photoId, kind, folder, count: count ?? null } }),
  blinkCheck: (projectId: string, photoIds?: string[]) =>
    invoke<BlinkReport>('blink_check', { input: { projectId, photoIds: photoIds ?? null } }),
};
