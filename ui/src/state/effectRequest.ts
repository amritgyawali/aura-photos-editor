import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';
import type { RetouchTool } from '../ipc/nativeRetouch';
import type { EFFECT_DEFAULTS, EffectTool } from '../ipc/retouchEffects';

/** A starting point a shortcut sets on top of a tool's own defaults. */
export type EffectPatch = Partial<typeof EFFECT_DEFAULTS[EffectTool]>;
/** A tool somebody chose in the Effects library, waiting for Retouch to open with it. ADR-0135. */
export type EffectRequest = { tool: RetouchTool; patch: EffectPatch; sequence: number };

/** How many recently used tools the library remembers. */
export const RECENT_LIMIT = 8;

type EffectRequestState = {
  request: EffectRequest | null;
  sequence: number;
  /** Most recent first. A per-viewer convenience, kept in this browser only. */
  recent: RetouchTool[];
  favourites: RetouchTool[];
  ask: (tool: RetouchTool, patch?: EffectPatch) => void;
  /** Clears the request Retouch has acted on, and only that one. */
  consume: (sequence: number) => void;
  toggleFavourite: (tool: RetouchTool) => void;
};

export const useEffectRequest = create<EffectRequestState>()(persist((set) => ({
  request: null, sequence: 0, recent: [], favourites: [],
  ask: (tool, patch = {}) => set(state => ({
    sequence: state.sequence + 1,
    request: { tool, patch, sequence: state.sequence + 1 },
    recent: [tool, ...state.recent.filter(t => t !== tool)].slice(0, RECENT_LIMIT),
  })),
  consume: sequence => set(state => state.request?.sequence === sequence ? { request: null } : {}),
  toggleFavourite: tool => set(state => ({
    favourites: state.favourites.includes(tool) ? state.favourites.filter(t => t !== tool) : [...state.favourites, tool],
  })),
}), {
  name: 'aura-effects-library',
  // Storage can be unavailable (a private window, blocked site data); the library still works.
  storage: createJSONStorage(() => {
    try { return localStorage; } catch { return sessionStorage; }
  }),
  partialize: ({ recent, favourites }) => ({ recent, favourites }),
}));
