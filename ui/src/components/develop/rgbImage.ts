import type { RenderDto } from '../../ipc/types';
import { bmpDataUrl, identity, ownedImageUrl } from './imageUrls';

type Pixels = Pick<RenderDto, 'width' | 'height' | 'rgbBase64'>;

function decodeBase64(text: string): Uint8Array {
  const native = (Uint8Array as unknown as { fromBase64?: (s: string) => Uint8Array }).fromBase64;
  if (typeof native === 'function') return native(text);
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** The interleaved RGB of a render, or null when the payload is not exactly the pixels it claims. */
export function rgbBytes(render: Pixels): Uint8Array | null {
  const { width, height } = render;
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1) return null;
  let bytes: Uint8Array;
  try { bytes = decodeBase64(render.rgbBase64); }
  catch { return null; } // An invalid payload must not crash the editor.
  return bytes.length === width * height * 3 ? bytes : null;
}

/** Wrap interleaved RGB in a lossless browser-readable BMP file. */
export function rgbBmp(rgb: Uint8Array, width: number, height: number): Uint8Array<ArrayBuffer> {
  const stride = Math.ceil(width * 3 / 4) * 4;
  const bytes = new Uint8Array(54 + stride * height);
  const header = new DataView(bytes.buffer);
  bytes[0] = 66; bytes[1] = 77;
  header.setUint32(2, bytes.length, true);
  header.setUint32(10, 54, true);
  header.setUint32(14, 40, true);
  header.setInt32(18, width, true);
  header.setInt32(22, -height, true); // top-down rows, same as the renderer
  header.setUint16(26, 1, true);
  header.setUint16(28, 24, true);
  header.setUint32(34, stride * height, true);
  for (let y = 0; y < height; y++) {
    const row = 54 + y * stride;
    for (let x = 0; x < width; x++) {
      const from = (y * width + x) * 3;
      const to = row + x * 3;
      bytes[to] = rgb[from + 2] ?? 0;
      bytes[to + 1] = rgb[from + 1] ?? 0;
      bytes[to + 2] = rgb[from] ?? 0;
    }
  }
  return bytes;
}

function renderBmp(render: Pixels): Uint8Array<ArrayBuffer> | null {
  const rgb = rgbBytes(render);
  return rgb ? rgbBmp(rgb, render.width, render.height) : null;
}

/** Tint the backend's saved coverage on the displayed photo, as RGB. Zero coverage preserves the
 * displayed RGB exactly; strength controls visibility only and never changes the recipe. */
function coverageRgb(photo: Pixels, mask: Pixels, opacity: number, tint: [number, number, number],
  crop: [number, number, number, number]): Uint8Array | null {
  if (!Number.isFinite(opacity) || mask.width < 1 || mask.height < 1) return null;
  const rgb = rgbBytes(photo), coverage = rgbBytes(mask);
  if (!rgb || !coverage) return null;
  const bytes = new Uint8Array(rgb.length);
  const strength = Math.max(0, Math.min(1, opacity));
  // Coverage may be measured at a smaller size than the full-quality photograph, and over the
  // whole frame when the photograph shown is cropped; each pixel reads the coverage cell it
  // falls in, so the tint lines up at any size. ADR-0097, ADR-0102.
  const [cl, ct, cr, cb] = crop;
  const sx = mask.width * (cr - cl) / photo.width, sy = mask.height * (cb - ct) / photo.height;
  const ox = mask.width * cl, oy = mask.height * ct;
  for (let y = 0; y < photo.height; y++) {
    const row = Math.max(0, Math.min(mask.height - 1, Math.floor(oy + (y + .5) * sy))) * mask.width;
    for (let x = 0; x < photo.width; x++) {
      const cell = (row + Math.max(0, Math.min(mask.width - 1, Math.floor(ox + (x + .5) * sx)))) * 3;
      const alpha = (coverage[cell] ?? 0) / 255 * strength;
      const at = (y * photo.width + x) * 3;
      for (let c = 0; c < 3; c++) bytes[at + c] = Math.round((rgb[at + c] ?? 0) * (1 - alpha) + (tint[c] ?? 0) * alpha);
    }
  }
  return bytes;
}

/** Tint every pixel the saved retouch actually changed, as RGB: `after` against `before`. A
 * change of a few code values or more shows at full tint, so a healed pimple is a visible dot. */
function changesRgb(before: Pixels, after: Pixels, opacity: number): Uint8Array | null {
  if (before.width !== after.width || before.height !== after.height || !Number.isFinite(opacity)) return null;
  const a = rgbBytes(after), b = rgbBytes(before);
  if (!a || !b) return null;
  const bytes = new Uint8Array(a.length);
  const tint = [255, 120, 40];
  const strength = Math.max(0, Math.min(1, opacity));
  for (let p = 0; p < a.length; p += 3) {
    let diff = 0;
    for (let c = 0; c < 3; c++) diff = Math.max(diff, Math.abs((a[p + c] ?? 0) - (b[p + c] ?? 0)));
    const alpha = Math.max(0, Math.min(1, (diff - 1) / 8)) * strength;
    for (let c = 0; c < 3; c++) bytes[p + c] = Math.round((a[p + c] ?? 0) * (1 - alpha) + (tint[c] ?? 0) * alpha);
  }
  return bytes;
}

const FULL_FRAME: [number, number, number, number] = [0, 0, 1, 1];
const COVERAGE_TINT: [number, number, number] = [40, 220, 190];

/** The backend's coverage tinted on the photo, as a `data:` URL. */
export function coverageDataUrl(photo: Pixels, mask: Pixels, opacity: number,
  tint: [number, number, number] = COVERAGE_TINT, crop: [number, number, number, number] = FULL_FRAME): string | null {
  const rgb = coverageRgb(photo, mask, opacity, tint, crop);
  return rgb ? bmpDataUrl(rgbBmp(rgb, photo.width, photo.height)) : null;
}

/** The pixels a retouch changed, tinted, as a `data:` URL. */
export function changesDataUrl(before: Pixels, after: Pixels, opacity: number): string | null {
  const rgb = changesRgb(before, after, opacity);
  return rgb ? bmpDataUrl(rgbBmp(rgb, after.width, after.height)) : null;
}

/** A render as a `data:` URL. Prefer `renderImageUrl` for anything the size of a photograph. */
export function rgbDataUrl(render: Pixels): string | null {
  const bmp = renderBmp(render);
  return bmp ? bmpDataUrl(bmp) : null;
}

/** A render as an image URL owned by the render: made once, released with it. */
export function renderImageUrl(render: Pixels | null | undefined): string | null {
  return render ? ownedImageUrl(render, 'render', '', () => renderBmp(render)) : null;
}

/** `coverageDataUrl` as an image URL owned by the photo; a new opacity replaces the old one. */
export function coverageImageUrl(photo: Pixels, mask: Pixels, opacity: number,
  tint: [number, number, number] = COVERAGE_TINT, crop: [number, number, number, number] = FULL_FRAME): string | null {
  return ownedImageUrl(photo, 'coverage', `${identity(mask)}|${opacity}|${tint.join(',')}|${crop.join(',')}`, () => {
    const rgb = coverageRgb(photo, mask, opacity, tint, crop);
    return rgb ? rgbBmp(rgb, photo.width, photo.height) : null;
  });
}

/** `changesDataUrl` as an image URL owned by the edited photo. */
export function changesImageUrl(before: Pixels, after: Pixels, opacity: number): string | null {
  return ownedImageUrl(after, 'changes', `${identity(before)}|${opacity}`, () => {
    const rgb = changesRgb(before, after, opacity);
    return rgb ? rgbBmp(rgb, after.width, after.height) : null;
  });
}
