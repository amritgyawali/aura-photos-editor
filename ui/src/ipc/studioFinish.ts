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
  // ADR-0150
  eyeHeight: number; eyeTilt: number; innerCorner: number; outerCorner: number; eyeSymmetry: number;
  noseBridge: number; noseTip: number; noseLift: number; nostrils: number; cheekApples: number;
  upperLip: number; lowerLip: number; mouthHeight: number; browHeight: number; browTilt: number;
  browThickness: number; temples: number; faceLength: number; chinWidth: number; hairVolume: number;
};
export type BodyShape = {
  slim: number; waist: number; hips: number; arms: number; shoulders: number; legs: number; neck: number;
  // ADR-0150
  thighs: number; calves: number; belly: number; hipLift: number; shoulderSlope: number;
  torso: number; height: number; neckWidth: number; doubleChin: number; headLevel: number;
};
export type LiquifyMode = 'push' | 'bloat' | 'pinch' | 'restore';
export type LiquifyStroke = { mode: LiquifyMode; radius: number; strength: number; points: [number, number][] };
export type BackgroundMode = 'colour' | 'gradient' | 'blur' | 'sky' | 'keep_shadows' | 'cloud_sky';
/** Colours are sRGB in 0..1, as a colour picker shows them. */
export type Background = { mode: BackgroundMode; colour: [number, number, number]; colour2: [number, number, number]; amount: number; feather: number };
export type Tint = { colour: [number, number, number]; amount: number };
export type FeatureColours = { hair: Tint | null; eyes: Tint | null; lips: Tint | null; blush: Tint | null; eyeshadow: Tint | null; eyebrows: Tint | null };
/**
 * Portrait light (ADR-0127): a key light shaped by each head, a fill on the shadow side,
 * catchlights in eyes without them and a soft glow. Amounts are 0..100 and zero means untouched;
 * direction (-180..180, negative is the photograph's left) and height (-30..80) place the key light.
 */
export type PortraitLight = { key: number; direction: number; height: number; fill: number; catchlights: number; glow: number };
/** ADR-0150: measured skin finishing on each face; every value 0..100. */
export type SkinFinish = {
  aegyoSal: number; lipLines: number; glow: number; stubble: number; rosy: number;
  teethEvening: number; featureSharpen: number; beardDefine: number; colourSpill: number; pores: number;
};
/** ADR-0150: makeup placed from each face's landmarks; every value 0..100. */
export type MakeupFinish = {
  eyeliner: number; eyelinerWing: number; lashes: number; lowerLashes: number; lipLiner: number;
  lipOmbre: number; highlighter: number; bronzer: number; enhance: number; browFill: number;
};
/** ADR-0150: hair and body skin; every value 0..100. */
export type HairBodyFinish = { gloss: number; parting: number; richness: number; edge: number; definition: number };
/** ADR-0150: what people are wearing; every value 0..100. */
export type AttireFinish = {
  gold: number; redRecovery: number; veilClarity: number; embroidery: number; garmentEdge: number;
  mehndi: number; whiteGarment: number; darkGarment: number; outfitColour: number;
};
/** ADR-0150: the background behind the people. */
export type SceneFinish = {
  backdropGlow: number; saturation: number; brightness: number; warmth: number;
  depthBlur: number; denoise: number; lightWrap: number;
};
/** ADR-0150: the people as a whole. */
export type SubjectFinish = {
  exposure: number; warmth: number; clarity: number; groupBalance: number; groupSkin: number;
  relight: number; relightDirection: number; harshShadows: number; backlight: number;
  faceVignette: number; faceBrightness: number;
};
export type StudioFinish = {
  face: FaceShape; body: BodyShape; liquify?: LiquifyStroke[]; backgroundRepair?: number; background?: Background | null; colours: FeatureColours; light: PortraitLight;
  skin: SkinFinish; makeup: MakeupFinish; hair: HairBodyFinish; attire: AttireFinish; scene: SceneFinish; subject: SubjectFinish;
  /** The faces the face, skin and makeup tools apply to, as normalised points; empty is every face. */
  faces: [number, number][];
  /** The people as the learned selection found them, frozen by the backend; carried unchanged. */
  subjectMatte?: unknown;
};
export type StudioFinishDto = { finish: StudioFinish; recipe: RecipeDto };
/** What Auto light measured and proposes. Nothing is stored until the photographer saves it. */
export type LightSuggestion = { light: PortraitLight; faces: number; contrast: number | null; reasons: string[] };

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

/** One slider of the ADR-0150 groups: key, label, range and what it does. */
export type Slider<K extends string> = readonly [K, string, number, number, string];

/** The twenty ADR-0150 face sliders, grouped as the panel shows them. */
export const MORE_FACE_SLIDERS: ReadonlyArray<readonly [string, ReadonlyArray<Slider<keyof FaceShape>>]> = [
  ['Eyes', [
    ['eyeHeight', 'Eye height', -100, 100, 'Raises or lowers the eyes.'],
    ['eyeTilt', 'Eye tilt', -100, 100, 'Lifts or drops the outer corners.'],
    ['innerCorner', 'Inner corners', -100, 100, 'Opens the inner corners toward the nose.'],
    ['outerCorner', 'Outer corners', -100, 100, 'Lengthens the outer corners.'],
    ['eyeSymmetry', 'Eye symmetry', 0, 100, 'Opens the smaller eye toward the larger, measured on each face.'],
  ]],
  ['Nose', [
    ['noseBridge', 'Nose bridge', -100, 100, 'Narrows the bridge of the nose.'],
    ['noseTip', 'Nose tip', -100, 100, 'Makes the tip smaller.'],
    ['noseLift', 'Nose lift', -100, 100, 'Lifts the tip.'],
    ['nostrils', 'Nostrils', -100, 100, 'Narrows the nostrils.'],
  ]],
  ['Mouth and brows', [
    ['upperLip', 'Upper lip', -100, 100, 'Makes the upper lip fuller or thinner.'],
    ['lowerLip', 'Lower lip', -100, 100, 'Makes the lower lip fuller or thinner.'],
    ['mouthHeight', 'Mouth position', -100, 100, 'Raises the mouth, shortening the space under the nose.'],
    ['browHeight', 'Brow height', -100, 100, 'Raises or lowers the eyebrows.'],
    ['browTilt', 'Brow arch', -100, 100, 'Lifts the tails of the eyebrows.'],
    ['browThickness', 'Brow thickness', -100, 100, 'Makes the eyebrows thicker or thinner.'],
  ]],
  ['Face shape', [
    ['cheekApples', 'Cheek apples', -100, 100, 'Fuller apples of the cheeks.'],
    ['temples', 'Temples', -100, 100, 'Fills out hollow temples.'],
    ['faceLength', 'Lower face length', -100, 100, 'Lengthens or shortens the lower face.'],
    ['chinWidth', 'Chin width', -100, 100, 'Narrows the chin.'],
    ['hairVolume', 'Hair volume', -100, 100, 'Adds volume at the crown.'],
  ]],
] as const;

export const MORE_BODY_SLIDERS: ReadonlyArray<Slider<keyof BodyShape>> = [
  ['thighs', 'Thighs', -100, 100, 'Slims the thighs.'],
  ['calves', 'Calves', -100, 100, 'Slims the calves.'],
  ['belly', 'Flat stomach', -100, 100, 'Flattens the stomach on the side the silhouette shows it.'],
  ['hipLift', 'Hip lift', -100, 100, 'Lifts the hips.'],
  ['shoulderSlope', 'Shoulder line', -100, 100, 'Squares or lets slope the shoulders.'],
  ['torso', 'Torso length', -100, 100, 'Lengthens the torso between the shoulders and the waist.'],
  ['height', 'Height', -100, 100, 'Makes the whole figure below the shoulders taller.'],
  ['neckWidth', 'Neck slim', -100, 100, 'Slims the neck.'],
  ['doubleChin', 'Double chin', 0, 100, 'Lifts a double chin and evens its fold.'],
  ['headLevel', 'Level head', 0, 100, 'Turns a tilted head toward level, measured from the eyes.'],
] as const;

export const SKIN_SLIDERS: ReadonlyArray<Slider<keyof SkinFinish>> = [
  ['aegyoSal', 'Aegyo-sal (eye smile)', 0, 100, 'A soft lift along each lower lid, with a faint shade under it.'],
  ['lipLines', 'Lip lines', 0, 100, 'Smooths fine vertical lines across the lips.'],
  ['glow', 'Dewy glow', 0, 100, 'A glow on the forehead, nose bridge, cheekbones and chin, where light already falls.'],
  ['stubble', 'Stubble shadow', 0, 100, 'Evens the blue-grey shadow of shaved stubble toward the cheeks. A beard is kept.'],
  ['rosy', 'Rosy complexion', 0, 100, 'Turns sallow skin a little toward rose, keeping its brightness.'],
  ['teethEvening', 'Even teeth', 0, 100, 'Brings a darker or yellower tooth toward the others.'],
  ['featureSharpen', 'Sharpen features', 0, 100, 'Sharpens eyes, lashes, brows and lips, never the skin.'],
  ['beardDefine', 'Beard definition', 0, 100, 'Deepens and defines a beard or moustache.'],
  ['colourSpill', 'Clothing colour on skin', 0, 100, 'Takes out the colour a red sari or a green wall casts onto skin.'],
  ['pores', 'Refine pores', 0, 100, 'Reduces pores on the nose and cheeks, keeping finer grain.'],
] as const;

export const MAKEUP_SLIDERS: ReadonlyArray<Slider<keyof MakeupFinish>> = [
  ['eyeliner', 'Eyeliner', 0, 100, 'A line along the upper lash line, thicker toward the outer corner.'],
  ['eyelinerWing', 'Eyeliner wing', 0, 100, 'Flicks the liner up and out past the outer corner.'],
  ['lashes', 'Fuller lashes', 0, 100, 'Darkens and fills the upper lashes already there.'],
  ['lowerLashes', 'Lower lashes', 0, 100, 'Defines the lower lashes.'],
  ['lipLiner', 'Lip liner', 0, 100, 'Defines the lips\' outer border in a deeper shade of their own colour.'],
  ['lipOmbre', 'Ombre lips', 0, 100, 'Deeper colour at the centre of the lips, softer at the edge.'],
  ['highlighter', 'Highlighter', 0, 100, 'A soft sheen on cheekbones, nose bridge, brow bone and cupid\'s bow.'],
  ['bronzer', 'Bronzer', 0, 100, 'A warm tone around the face and under the cheekbones.'],
  ['enhance', 'Enhance makeup', 0, 100, 'Makes lipstick, eyeshadow and blush already worn richer.'],
  ['browFill', 'Fill brows', 0, 100, 'Fills sparse eyebrows with the colour of their own darkest hairs.'],
] as const;

export const HAIR_SLIDERS: ReadonlyArray<Slider<keyof HairBodyFinish>> = [
  ['gloss', 'Hair shine', 0, 100, 'Shine along the highlights the hair already has.'],
  ['parting', 'Fill parting', 0, 100, 'Fills a bright parting line with the hair\'s own colour.'],
  ['richness', 'Hair colour depth', 0, 100, 'Deeper, richer hair colour without changing its hue.'],
  ['edge', 'Calm hair edge', 0, 100, 'Smooths a frizzy, haloed hair edge against the background.'],
  ['definition', 'Body definition', 0, 100, 'Brings forward the muscle, collarbone and arm shape already there.'],
] as const;

export const ATTIRE_SLIDERS: ReadonlyArray<Slider<keyof AttireFinish>> = [
  ['gold', 'Gold jewellery', 0, 100, 'Richer, crisper gold, never on skin.'],
  ['redRecovery', 'Recover red fabric', 0, 100, 'Brings back the weave in red saris and veils whose colour clipped.'],
  ['veilClarity', 'Veil clarity', 0, 100, 'Clears a face seen through a net veil. On a bare face it sets the black point.'],
  ['embroidery', 'Embroidery detail', 0, 100, 'Brings embroidery, beadwork and lace forward; plain fabric is left.'],
  ['garmentEdge', 'Smooth garment edges', 0, 100, 'Smooths a garment\'s ragged silhouette along its edge.'],
  ['mehndi', 'Deepen mehndi', 0, 100, 'Deepens henna on hands and arms.'],
  ['whiteGarment', 'Clean white clothes', 0, 100, 'Removes the colour cast from white clothes, keeping their folds.'],
  ['darkGarment', 'Dark suit detail', 0, 100, 'Lifts detail out of crushed black fabric, which stays black.'],
  ['outfitColour', 'Outfit colour', 0, 100, 'More colour in the clothing, none in the skin.'],
] as const;

export const SCENE_SLIDERS: ReadonlyArray<Slider<keyof SceneFinish>> = [
  ['backdropGlow', 'Backdrop light', 0, 100, 'A soft pool of light on the backdrop behind each head.'],
  ['saturation', 'Background saturation', -100, 100, 'Background colour only; negative toward grey.'],
  ['brightness', 'Background brightness', -100, 100, 'Background brightness only, up to a stop.'],
  ['warmth', 'Background warmth', -100, 100, 'Background warmth only.'],
  ['depthBlur', 'Depth blur', 0, 100, 'A blur that grows with distance from the people.'],
  ['denoise', 'Background noise', 0, 100, 'Noise reduction on the background only.'],
  ['lightWrap', 'Light wrap', 0, 100, 'The background\'s light wrapped onto the people\'s edges.'],
] as const;

export const SUBJECT_SLIDERS: ReadonlyArray<Slider<keyof SubjectFinish>> = [
  ['exposure', 'People exposure', -100, 100, 'The people\'s brightness, up to a stop, background unchanged.'],
  ['warmth', 'People warmth', -100, 100, 'The people\'s warmth only.'],
  ['clarity', 'People clarity', -100, 100, 'Local contrast on the people; skin takes a quarter of it.'],
  ['faceBrightness', 'Face brightness', -100, 100, 'The faces alone.'],
  ['groupBalance', 'Balance group faces', 0, 100, 'Lifts darker faces in a group toward the group\'s median face.'],
  ['groupSkin', 'Match group skin colour', 0, 100, 'Pulls each face\'s skin colour toward the group\'s median.'],
  ['harshShadows', 'Soften harsh shadows', 0, 100, 'Softens hard shadows across faces.'],
  ['backlight', 'Backlight rescue', 0, 100, 'Lifts a dark, backlit subject; the bright background stays.'],
  ['relight', 'Relight', 0, 100, 'A soft directional light across the whole person.'],
  ['relightDirection', 'Relight direction', -180, 180, 'Where that light comes from; negative is the photograph\'s left.'],
  ['faceVignette', 'Face-aware vignette', 0, 100, 'Darker corners that never reach a face.'],
] as const;

export const COLOUR_FEATURES: ReadonlyArray<readonly [keyof FeatureColours, string, string]> = [
  ['hair', 'Hair colour', '#7a3b1e'], ['eyes', 'Eye colour', '#3f6f9f'], ['lips', 'Lipstick', '#b0303f'],
  ['blush', 'Blush', '#e48a8a'], ['eyeshadow', 'Eyeshadow', '#8a5a7a'], ['eyebrows', 'Brow colour', '#3a2a20'],
] as const;

export const LIGHT_SLIDERS: ReadonlyArray<readonly [keyof PortraitLight, string, number, number]> = [
  ['key', 'Key light', 0, 100], ['direction', 'Light direction', -180, 180], ['height', 'Light height', -30, 80],
  ['fill', 'Fill light', 0, 100], ['catchlights', 'Catchlights', 0, 100], ['glow', 'Glow', 0, 100],
] as const;

export const NEUTRAL_LIGHT: PortraitLight = { key: 0, direction: -45, height: 35, fill: 0, catchlights: 0, glow: 0 };
export const NEUTRAL_FACE: FaceShape = {
  slim: 0, jaw: 0, chin: 0, forehead: 0, cheekbones: 0, eyeSize: 0, eyeDistance: 0, noseWidth: 0, noseLength: 0, mouthWidth: 0, lips: 0, smile: 0, headSize: 0,
  eyeHeight: 0, eyeTilt: 0, innerCorner: 0, outerCorner: 0, eyeSymmetry: 0, noseBridge: 0, noseTip: 0, noseLift: 0, nostrils: 0, cheekApples: 0,
  upperLip: 0, lowerLip: 0, mouthHeight: 0, browHeight: 0, browTilt: 0, browThickness: 0, temples: 0, faceLength: 0, chinWidth: 0, hairVolume: 0,
};
export const NEUTRAL_BODY: BodyShape = {
  slim: 0, waist: 0, hips: 0, arms: 0, shoulders: 0, legs: 0, neck: 0,
  thighs: 0, calves: 0, belly: 0, hipLift: 0, shoulderSlope: 0, torso: 0, height: 0, neckWidth: 0, doubleChin: 0, headLevel: 0,
};
const zeroes = <K extends string>(list: ReadonlyArray<Slider<K>>): Record<K, number> =>
  Object.fromEntries(list.map(([key]) => [key, 0])) as Record<K, number>;
export const NEUTRAL_SKIN: SkinFinish = zeroes(SKIN_SLIDERS);
export const NEUTRAL_MAKEUP: MakeupFinish = zeroes(MAKEUP_SLIDERS);
export const NEUTRAL_HAIR: HairBodyFinish = zeroes(HAIR_SLIDERS);
export const NEUTRAL_ATTIRE: AttireFinish = zeroes(ATTIRE_SLIDERS);
export const NEUTRAL_SCENE: SceneFinish = zeroes(SCENE_SLIDERS);
export const NEUTRAL_SUBJECT: SubjectFinish = { ...zeroes(SUBJECT_SLIDERS), relightDirection: -45 };
export const NEUTRAL_FINISH: StudioFinish = {
  face: NEUTRAL_FACE, body: NEUTRAL_BODY, liquify: [], backgroundRepair: 0, background: null,
  colours: { hair: null, eyes: null, lips: null, blush: null, eyeshadow: null, eyebrows: null },
  light: NEUTRAL_LIGHT,
  skin: NEUTRAL_SKIN, makeup: NEUTRAL_MAKEUP, hair: NEUTRAL_HAIR, attire: NEUTRAL_ATTIRE, scene: NEUTRAL_SCENE, subject: NEUTRAL_SUBJECT,
  faces: [], subjectMatte: null,
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
    backgroundRepair: value?.backgroundRepair ?? 0,
    background: value?.background ?? null,
    colours: { ...NEUTRAL_FINISH.colours, ...(value?.colours ?? {}) },
    light: { ...NEUTRAL_LIGHT, ...(value?.light ?? {}) },
    skin: { ...NEUTRAL_SKIN, ...(value?.skin ?? {}) },
    makeup: { ...NEUTRAL_MAKEUP, ...(value?.makeup ?? {}) },
    hair: { ...NEUTRAL_HAIR, ...(value?.hair ?? {}) },
    attire: { ...NEUTRAL_ATTIRE, ...(value?.attire ?? {}) },
    scene: { ...NEUTRAL_SCENE, ...(value?.scene ?? {}) },
    subject: { ...NEUTRAL_SUBJECT, ...(value?.subject ?? {}) },
    faces: value?.faces ?? [],
    subjectMatte: value?.subjectMatte ?? null,
  };
}

export const studioFinish = {
  get: (projectId: string, photoId: string) =>
    invoke<StudioFinishDto>('studio_finish', { input: { projectId, photoId } }),
  save: (projectId: string, photoId: string, finish: StudioFinish, label: string) =>
    invoke<StudioFinishDto>('save_studio_finish', { input: { projectId, photoId, finish, label } }),
  /** Auto light: measure how each face is lit and propose a light. Stores nothing. */
  suggestLight: (projectId: string, photoId: string) =>
    invoke<LightSuggestion>('suggest_portrait_light', { input: { projectId, photoId } }),
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
  { id: 'studio-light', name: 'Studio portrait light', hint: 'A soft key light from the left with catchlights, as a studio softbox gives.',
    apply: f => ({ ...f, light: { ...f.light, key: 35, direction: -45, height: 35, fill: 10, catchlights: 60 } }) },
  { id: 'dreamy-glow', name: 'Dreamy glow', hint: 'A soft bloom from the brightest parts of the photograph.',
    apply: f => ({ ...f, light: { ...f.light, glow: 40 } }) },
  { id: 'sunset-sky', name: 'Sunset sky', hint: 'A warm golden-hour sky.',
    apply: f => ({ ...f, background: { mode: 'sky', colour: [0.36, 0.33, 0.6], colour2: [0.98, 0.66, 0.4], amount: 100, feather: 40 } }) },
  // ADR-0150
  { id: 'bridal-beauty', name: 'Bridal beauty', hint: 'Aegyo-sal, glow, fuller lashes, liner, highlighter and rich gold, held soft.',
    apply: f => ({ ...f, skin: { ...f.skin, aegyoSal: 30, glow: 30, featureSharpen: 30, pores: 30 }, makeup: { ...f.makeup, lashes: 40, eyeliner: 25, highlighter: 25 }, attire: { ...f.attire, gold: 50, redRecovery: 40, embroidery: 30 } }) },
  { id: 'groom-clean', name: 'Groom clean-up', hint: 'Evens stubble shadow, defines the beard, cleans white and dark clothes.',
    apply: f => ({ ...f, skin: { ...f.skin, stubble: 50, beardDefine: 30, featureSharpen: 25 }, attire: { ...f.attire, whiteGarment: 50, darkGarment: 40 } }) },
  { id: 'group-balance', name: 'Family group balance', hint: 'Lifts darker faces toward the group, matches skin colour, softens harsh shadows.',
    apply: f => ({ ...f, subject: { ...f.subject, groupBalance: 60, groupSkin: 40, harshShadows: 30 } }) },
  { id: 'subject-pop', name: 'Subject pop', hint: 'People a little brighter and clearer, the background quieter, with a face-aware vignette.',
    apply: f => ({ ...f, subject: { ...f.subject, exposure: 15, clarity: 20, faceVignette: 30 }, scene: { ...f.scene, saturation: -20, brightness: -15, depthBlur: 25 } }) },
  { id: 'cloud-sky', name: 'Cloudy blue sky', hint: 'A blue sky with soft clouds where the sky is open.',
    apply: f => ({ ...f, background: { mode: 'cloud_sky', colour: [0.24, 0.48, 0.86], colour2: [0.74, 0.86, 0.97], amount: 100, feather: 40 } }) },
];
