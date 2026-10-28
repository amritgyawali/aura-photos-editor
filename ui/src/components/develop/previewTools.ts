import type { RenderDto } from '../../ipc/types';
import { bmpDataUrl, ownedImageUrl } from './imageUrls';
import { renderImageUrl, rgbBmp, rgbBytes } from './rgbImage';

export type ClippingMode = 'off' | 'shadows' | 'highlights' | 'both';

function clippingBmp(render: RenderDto, mode: Exclude<ClippingMode, 'off'>): Uint8Array<ArrayBuffer> | null {
  const pixels = render.width * render.height;
  if (!Number.isSafeInteger(pixels) || pixels < 1 || pixels > 4_000_000) return null;
  const source = rgbBytes(render);
  if (!source) return null;
  const bytes = new Uint8Array(source.length);
  for (let p = 0; p < pixels; p++) {
    const at = p * 3;
    const r = source[at] ?? 0, g = source[at + 1] ?? 0, b = source[at + 2] ?? 0;
    const shadow = mode !== 'highlights' && Math.max(r, g, b) <= 2;
    const highlight = mode !== 'shadows' && Math.max(r, g, b) >= 253;
    // Distinct hatch patterns supplement the blue/red legend.
    const stripe = ((p % render.width) + Math.floor(p / render.width)) % 8 < 2;
    bytes.set(highlight ? (stripe ? [255, 255, 255] : [255, 32, 48]) :
      shadow ? (stripe ? [0, 0, 0] : [32, 112, 255]) : [r, g, b], at);
  }
  return rgbBmp(bytes, render.width, render.height);
}

/** Mark display clipping without changing the saved recipe or exported pixels. The picture is
 * owned by the render it marks, and choosing another mode replaces it. */
export function clippingPreview(render: RenderDto, mode: ClippingMode): string | null {
  if (mode === 'off') return renderImageUrl(render);
  return ownedImageUrl(render, 'clipping', mode, () => clippingBmp(render, mode));
}

/** `clippingPreview` as a `data:` URL, for a caller that keeps only the string. */
export function clippingDataUrl(render: RenderDto, mode: Exclude<ClippingMode, 'off'>): string | null {
  const bmp = clippingBmp(render, mode);
  return bmp ? bmpDataUrl(bmp) : null;
}

/** Map a pointer in an object-fit:contain element to the original image; reject letterboxing. */
export function imagePoint(x: number, y: number, boxWidth: number, boxHeight: number, aspect: number): [number, number] | null {
  if (![x, y, boxWidth, boxHeight, aspect].every(Number.isFinite) || boxWidth <= 0 || boxHeight <= 0 || aspect <= 0) return null;
  const width = Math.min(boxWidth, boxHeight * aspect), height = width / aspect;
  const px = (x - (boxWidth - width) / 2) / width, py = (y - (boxHeight - height) / 2) / height;
  return px >= 0 && px <= 1 && py >= 0 && py <= 1 ? [px, py] : null;
}
