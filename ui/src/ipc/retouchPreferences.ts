import type { RecipeDto } from './types';
import { DEFAULT_AUTO_RETOUCH, DEFAULT_RETOUCH_SETTINGS, type AutoRetouchOptions, type RetouchSettings } from './nativeRetouch';

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const signed = new Set<keyof RetouchSettings>(['skinBrightness', 'skinWarmth', 'skinTint']);

/** Restore only known, valid settings from this photograph's recipe extension. */
export function readRetouchPreferences(recipe?: RecipeDto | null): AutoRetouchOptions {
  const settings = { ...DEFAULT_RETOUCH_SETTINGS };
  const options: AutoRetouchOptions = { ...DEFAULT_AUTO_RETOUCH, settings };
  try {
    const body: unknown = JSON.parse(recipe?.body ?? '{}');
    const report = record(body) ? body.studio_portrait_auto_v1 : null;
    const saved = record(report) ? report.options : null;
    if (!record(saved)) return options;
    if (typeof saved.intensity === 'number' && Number.isFinite(saved.intensity)) {
      options.intensity = Math.max(.25, Math.min(1.5, saved.intensity));
    }
    if (saved.scope === 'face' || saved.scope === 'body' || saved.scope === 'face_and_body') options.scope = saved.scope;
    for (const key of ['blemishes', 'refine', 'eyes', 'teeth'] as const) {
      if (typeof saved[key] === 'boolean') options[key] = saved[key];
    }
    if (record(saved.settings)) {
      for (const key of Object.keys(settings) as (keyof RetouchSettings)[]) {
        const value = saved.settings[key];
        if (typeof settings[key] === 'boolean' && typeof value === 'boolean') Object.assign(settings, { [key]: value });
        else if (typeof settings[key] === 'number' && typeof value === 'number' && Number.isFinite(value)) {
          const bounded = key === 'maxSpots' ? Math.round(Math.max(1, Math.min(24, value)))
            : Math.max(signed.has(key) ? -1 : 0, Math.min(1, value));
          Object.assign(settings, { [key]: bounded });
        }
      }
    }
  } catch { /* Invalid or legacy extensions use defaults without breaking the editor. */ }
  return options;
}
