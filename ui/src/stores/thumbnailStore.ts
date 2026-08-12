import { create } from 'zustand';

import { api, inTauri } from '../ipc/client';
import type { PreviewPayload } from '../ipc/types';

/**
 * The grid's view of the preview pyramid.
 *
 * Three problems have to be solved here, and each one has bitten every product
 * in this category:
 *
 * 1. **Do not ask twice.** A grid re-renders constantly; without an in-flight
 *    set, scrolling one screen would queue the same decode a dozen times.
 * 2. **Give up on what left the screen.** Work for cells the user has scrolled
 *    past is cancelled, not merely deprioritised, so the visible row is never
 *    behind a queue of frames nobody is looking at.
 * 3. **Do not keep every bitmap forever.** Four thousand 512 px data URLs is
 *    well over a gigabyte in the web view, so the store is an LRU with a hard
 *    entry ceiling.
 *
 * Tier upgrades are progressive: the thumbnail arrives first and is replaced in
 * place by the proxy when the loupe asks for one, so a cell never blanks.
 */

/** How many decoded previews to keep in the web view at once. */
export const MAX_ENTRIES = 600;

export type ThumbnailEntry = {
  dataUrl: string;
  tier: number;
  width: number;
  height: number;
  source: string;
  /** Access counter, used for least-recently-used eviction. */
  used: number;
};

export type ThumbnailState = {
  entries: Map<string, ThumbnailEntry>;
  inFlight: Set<string>;
  failed: Map<string, string>;
  counter: number;

  get: (photoId: string) => ThumbnailEntry | undefined;
  put: (photoId: string, payload: PreviewPayload) => void;
  markFailed: (photoId: string, message: string) => void;
  request: (projectId: string, photoId: string, level?: string) => Promise<void>;
  requestMany: (projectId: string, photoIds: string[], level?: string) => Promise<void>;
  release: (projectId: string, photoIds: string[]) => Promise<void>;
  clear: () => void;
};

/** Evict least-recently-used entries until the map fits the ceiling. */
function evict(entries: Map<string, ThumbnailEntry>): Map<string, ThumbnailEntry> {
  if (entries.size <= MAX_ENTRIES) {
    return entries;
  }
  const ordered = [...entries.entries()].sort((left, right) => left[1].used - right[1].used);
  const next = new Map(entries);
  for (const [id] of ordered.slice(0, entries.size - MAX_ENTRIES)) {
    next.delete(id);
  }
  return next;
}

export const useThumbnails = create<ThumbnailState>((set, get) => ({
  entries: new Map<string, ThumbnailEntry>(),
  inFlight: new Set<string>(),
  failed: new Map<string, string>(),
  counter: 0,

  get: (photoId) => {
    const found = get().entries.get(photoId);
    if (found) {
      // Touching on read is what makes the ceiling least-recently-used rather
      // than least-recently-decoded.
      set((state) => {
        const counter = state.counter + 1;
        const entries = new Map(state.entries);
        entries.set(photoId, { ...found, used: counter });
        return { entries, counter };
      });
    }
    return found;
  },

  put: (photoId, payload) =>
    set((state) => {
      const counter = state.counter + 1;
      const existing = state.entries.get(photoId);
      // A lower tier never overwrites a higher one: the loupe has already
      // upgraded this cell and the grid must not undo that.
      if (existing && existing.tier > payload.tier) {
        return {};
      }
      const entries = new Map(state.entries);
      entries.set(photoId, {
        dataUrl: payload.dataUrl,
        tier: payload.tier,
        width: payload.width,
        height: payload.height,
        source: payload.source,
        used: counter,
      });
      const inFlight = new Set(state.inFlight);
      inFlight.delete(photoId);
      const failed = new Map(state.failed);
      failed.delete(photoId);
      return { entries: evict(entries), inFlight, failed, counter };
    }),

  markFailed: (photoId, message) =>
    set((state) => {
      const inFlight = new Set(state.inFlight);
      inFlight.delete(photoId);
      const failed = new Map(state.failed);
      failed.set(photoId, message);
      return { inFlight, failed };
    }),

  request: async (projectId, photoId, level = 'thumb') => {
    const state = get();
    if (state.entries.has(photoId) || state.inFlight.has(photoId) || state.failed.has(photoId)) {
      return;
    }
    if (!inTauri()) {
      return;
    }
    set((current) => ({ inFlight: new Set(current.inFlight).add(photoId) }));
    try {
      const payload = await api.getPreview({
        projectId,
        photoId,
        level,
        priority: 'visible',
      });
      get().put(photoId, payload);
    } catch (error) {
      const message = error instanceof Error ? error.message : 'This photo could not be opened.';
      get().markFailed(photoId, message);
    }
  },

  requestMany: async (projectId, photoIds, level = 'thumb') => {
    const state = get();
    const wanted = photoIds.filter(
      (id) => !state.entries.has(id) && !state.inFlight.has(id) && !state.failed.has(id),
    );
    if (wanted.length === 0 || !inTauri()) {
      return;
    }
    // The batch goes to the background queue; visible cells still ask
    // individually and are served first.
    await api.prefetchPreviews({ projectId, photoIds: wanted, level });
  },

  release: async (projectId, photoIds) => {
    if (photoIds.length === 0 || !inTauri()) {
      return;
    }
    await api.cancelPreviews(projectId, photoIds);
    set((state) => {
      const inFlight = new Set(state.inFlight);
      for (const id of photoIds) {
        inFlight.delete(id);
      }
      return { inFlight };
    });
  },

  clear: () =>
    set({
      entries: new Map<string, ThumbnailEntry>(),
      inFlight: new Set<string>(),
      failed: new Map<string, string>(),
      counter: 0,
    }),
}));
