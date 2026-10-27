import { invoke } from './client';

export const AUTOMATIC_CATEGORIES = {
  blemishes: 'Blemish removal', smoothing: 'Skin smoothing', tone: 'Skin tone and light',
  wrinkles: 'Wrinkle correction', eyes: 'Eye and under-eye correction', teeth: 'Teeth correction',
  shine: 'Shine and glare correction', hair: 'Hair refinement', clothing: 'Clothing refinement', background: 'Background cleanup',
} as const;
export type AutomaticCategory = keyof typeof AUTOMATIC_CATEGORIES;
export type FinishStrengths = { overall: number; categories: Partial<Record<AutomaticCategory, number>> };
export type AutomaticFinish = {
  recipeHash: string; strengths: FinishStrengths;
  categories: { category: AutomaticCategory; operations: number }[]; newOperations: number;
};
export const automaticStrength = {
  read: (projectId: string, photoId: string) => invoke<AutomaticFinish>('automatic_finish_state', { input: { projectId, photoId } }),
  apply: (projectId: string, photoId: string, recipeHash: string, strengths: FinishStrengths) =>
    invoke<AutomaticFinish>('adjust_automatic_finish', { input: { projectId, photoId, recipeHash, strengths } }),
};
