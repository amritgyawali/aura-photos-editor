import type { RecipeDto } from '../../ipc/types';

export function portraitMessage(recipe: RecipeDto | null | undefined): string | null {
  if (!recipe?.body) return null;
  try {
    const report: unknown = JSON.parse(recipe.body).studio_portrait_auto_v1;
    if (report && typeof report === 'object' && 'message' in report && typeof report.message === 'string') return report.message;
  } catch { /* A legacy recipe may have no automatic portrait report. */ }
  return null;
}

export function PortraitAutoReport({ recipe }: { recipe: RecipeDto | null }) {
  const message = portraitMessage(recipe);
  return message ? <p className="lr-hint" role="status">Last automatic pass: {message}</p> : null;
}
