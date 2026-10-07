import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../ipc/client';
import type { RenderDto } from '../ipc/types';
import { rgbDataUrl } from '../components/develop/rgbImage';

/** How sharp a preview is: the original's own resolution, or the screen-sized first look. */
export type PreviewQuality = 'full' | 'fast';

type Entry = { image: RenderDto; quality: PreviewQuality; size: number };

/**
 * Previews this window has already received, kept for the whole session so a photograph opens
 * instantly again after moving to another section and back. ADR-0097.
 *
 * Keyed by the caller: the photograph, what is shown (edited, before, original) and the recipe
 * hash, so a changed edit is a different key and a stale picture is never returned. The backend
 * keeps its own copies in memory and on disk; this one saves the transfer and the decoding.
 * Bounded by the characters of image data held - the pixels and the displayable copy made from
 * them, about the same size again - least recently used out first. A 24-megapixel photograph
 * is about 200 MB of the budget; a 6-megapixel portrait about 50 MB.
 */
const BUDGET = 640_000_000;
const entries = new Map<string, Entry>();
let used = 0;
type Pixels = Pick<RenderDto, 'width' | 'height' | 'rgbBase64'>;
const sources = new WeakMap<Pixels, string | null>();

/** The cached preview for `key`, marking it as just used. */
export function cachedPreview(key: string): Entry | undefined {
  const entry = entries.get(key);
  if (entry) { entries.delete(key); entries.set(key, entry); }
  return entry;
}

/** Keep a preview. A full-quality one is never replaced by a fast one. */
export function rememberPreview(key: string, image: RenderDto, quality: PreviewQuality): void {
  const previous = entries.get(key);
  if (previous?.quality === 'full' && quality === 'fast') return;
  // A payload that is not exactly the pixels it claims is shown once, never kept.
  if (image.rgbBase64.length !== 4 * image.width * image.height) return;
  if (previous) { used -= previous.size; entries.delete(key); }
  const size = image.rgbBase64.length * 2;
  if (size > BUDGET) return;
  entries.set(key, { image, quality, size });
  used += size;
  for (const [oldest, entry] of entries) {
    if (used <= BUDGET) break;
    entries.delete(oldest);
    used -= entry.size;
  }
}

/** Forget everything (tests, or after the backend cache is purged). */
export function forgetPreviews(): void {
  entries.clear();
  used = 0;
}

/** The displayable image for a preview, built once per preview and reused. */
export function previewSource(image: Pixels | null | undefined): string | null {
  if (!image) return null;
  if (!sources.has(image)) sources.set(image, rgbDataUrl(image));
  return sources.get(image) ?? null;
}

export type ProgressivePreview = {
  image: RenderDto | null;
  quality: PreviewQuality | null;
  /** True while the full-quality preview is still being made. */
  upgrading: boolean;
  /** True when `image` belongs to the current key; false while the last one is kept on screen. */
  current: boolean;
  error: string | null;
};

/**
 * Show a preview at once, then at full quality.
 *
 * A full-quality preview already in the cache is shown immediately and nothing is asked for.
 * Otherwise the fast first look is requested, shown, and then replaced by the full-quality
 * preview, which is requested once the fast one has arrived. While a new
 * key loads, the last picture of the same `scope` (the photograph) stays on screen, so saving an
 * edit never flashes an empty canvas - but nothing from another photograph is ever shown.
 */
export function useProgressivePreview(key: string | null, scope: string,
  load: (quality: PreviewQuality) => Promise<RenderDto>, attempt = 0): ProgressivePreview {
  const loader = useRef(load);
  loader.current = load;
  const initial = (): ProgressivePreview => {
    const hit = key ? cachedPreview(key) : undefined;
    return { image: hit?.image ?? null, quality: hit?.quality ?? null, upgrading: Boolean(key) && hit?.quality !== 'full', current: Boolean(hit), error: null };
  };
  const [state, setState] = useState<ProgressivePreview>(initial);
  const shownScope = useRef(scope);
  useEffect(() => {
    const sameScope = shownScope.current === scope;
    shownScope.current = scope;
    if (!key) {
      setState(previous => sameScope ? { ...previous, upgrading: false, current: false } : { image: null, quality: null, upgrading: false, current: false, error: null });
      return;
    }
    let active = true;
    const hit = cachedPreview(key);
    if (hit?.quality === 'full') {
      setState({ image: hit.image, quality: 'full', upgrading: false, current: true, error: null });
      return () => { active = false; };
    }
    setState(previous => hit ? { image: hit.image, quality: hit.quality, upgrading: true, current: true, error: null }
      : sameScope ? { ...previous, upgrading: true, current: false, error: null } : { image: null, quality: null, upgrading: true, current: false, error: null });
    // The full-quality render is asked for once the quick look is on screen - not beside it,
    // where the two would share the processor and the quick look would arrive later - and
    // only while this is still the version being shown: a slider dragged on through several
    // versions renders a full-quality picture for none but the last.
    const full = () => {
      if (!active) return;
      loader.current('full').then(image => {
        rememberPreview(key, image, 'full');
        if (active) setState({ image, quality: 'full', upgrading: false, current: true, error: null });
      }).catch(cause => {
        if (active) setState(previous => ({ ...previous, upgrading: false, error: asIpcError(cause).message }));
      });
    };
    if (hit) full();
    else {
      loader.current('fast').then(image => {
        rememberPreview(key, image, 'fast');
        if (active) setState(previous => ({ ...previous, image, quality: 'fast', current: true }));
      }).catch(() => undefined).finally(full);
    }
    return () => { active = false; };
  }, [key, scope, attempt]);
  return state;
}
