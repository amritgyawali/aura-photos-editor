import type { RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';

export type ClippingMode = 'off' | 'shadows' | 'highlights' | 'both';

/** Mark display clipping without changing the saved recipe or exported pixels. */
export function clippingPreview(render: RenderDto, mode: ClippingMode): string | null {
  if (mode === 'off') return rgbDataUrl(render);
  const pixels = render.width * render.height;
  if (!Number.isSafeInteger(pixels) || pixels < 1 || pixels > 4_000_000) return null;
  let source: string;
  try { source = atob(render.rgbBase64); } catch { return null; }
  if (source.length !== pixels * 3) return null;
  const bytes = new Uint8Array(source.length);
  for (let p = 0; p < pixels; p++) {
    const at = p * 3;
    const r = source.charCodeAt(at), g = source.charCodeAt(at + 1), b = source.charCodeAt(at + 2);
    const shadow = mode !== 'highlights' && Math.max(r, g, b) <= 2;
    const highlight = mode !== 'shadows' && Math.max(r, g, b) >= 253;
    // Distinct hatch patterns supplement the blue/red legend.
    const stripe = ((p % render.width) + Math.floor(p / render.width)) % 8 < 2;
    bytes.set(highlight ? (stripe ? [255, 255, 255] : [255, 32, 48]) :
      shadow ? (stripe ? [0, 0, 0] : [32, 112, 255]) : [r, g, b], at);
  }
  const chunks: string[] = [];
  for (let at = 0; at < bytes.length; at += 8192) chunks.push(String.fromCharCode(...bytes.subarray(at, at + 8192)));
  return rgbDataUrl({ ...render, rgbBase64: btoa(chunks.join('')) });
}

/** Map a pointer in an object-fit:contain element to the original image; reject letterboxing. */
export function imagePoint(x: number, y: number, boxWidth: number, boxHeight: number, aspect: number): [number, number] | null {
  if (![x, y, boxWidth, boxHeight, aspect].every(Number.isFinite) || boxWidth <= 0 || boxHeight <= 0 || aspect <= 0) return null;
  const width = Math.min(boxWidth, boxHeight * aspect), height = width / aspect;
  const px = (x - (boxWidth - width) / 2) / width, py = (y - (boxHeight - height) / 2) / height;
  return px >= 0 && px <= 1 && py >= 0 && py <= 1 ? [px, py] : null;
}
