import type { RenderDto } from '../../ipc/types';
export type Crop = [number, number, number, number];
export type FramingFace = { bounds: Crop; eyes: [[number, number], [number, number]]; eyesMeasured: number; confidence: number };
export type FramingContext = { width: number; height: number; recipeHash: string; faces: FramingFace[]; subjectBounds: Crop | null; subjectAxis: number | null; reasons: string[] };
export function validCrop(crop: Crop): boolean {
  return crop.every(v => Number.isFinite(v) && v >= 0 && v <= 1) && crop[2] - crop[0] >= .001 && crop[3] - crop[1] >= .001;
}
export function ratioCrop(width: number, height: number, ratio: number, anchor = [.5, .5], zoom = 1): Crop {
  if (![width, height, ratio, zoom, ...anchor].every(Number.isFinite) || Math.min(width, height, ratio, zoom) <= 0 || zoom > 100) throw new Error('Enter positive dimensions and a valid crop scale.');
  const aspect = width / height;
  const w = Math.min(1, ratio / aspect) / zoom, h = Math.min(1, aspect / ratio) / zoom;
  const l = Math.max(0, Math.min(1 - w, (anchor[0] ?? .5) - w / 2));
  const t = Math.max(0, Math.min(1 - h, (anchor[1] ?? .5) - h / 2));
  return [l, t, l + w, t + h];
}
export function pixelCrop(width: number, height: number, pixels: [number, number], anchor = [.5, .5]): Crop {
  if (!pixels.every(v => Number.isSafeInteger(v) && v > 0) || pixels[0] > width || pixels[1] > height) throw new Error('Pixel crop must fit inside the original without upscaling.');
  const w = pixels[0] / width, h = pixels[1] / height;
  const l = Math.max(0, Math.min(1 - w, (anchor[0] ?? .5) - w / 2));
  const t = Math.max(0, Math.min(1 - h, (anchor[1] ?? .5) - h / 2));
  return [l, t, l + w, t + h];
}
export function printPixels(mm: [number, number], ppi: number): [number, number] {
  if (![...mm, ppi].every(v => Number.isFinite(v) && v > 0) || ppi > 1200 || mm.some(v => v > 1000)) throw new Error('Use dimensions up to 1000 mm and resolution up to 1200 PPI.');
  const pixels: [number, number] = [Math.round(mm[0] / 25.4 * ppi), Math.round(mm[1] / 25.4 * ppi)];
  if (pixels.some(v => v < 1 || v > 30000)) throw new Error('Print pixel dimensions must be between 1 and 30000.');
  return pixels;
}
export function faceFrame(context: FramingContext, face: FramingFace, ratio: number, headShare: number, topMargin: number, horizontal: number): Crop {
  if (![headShare, topMargin, horizontal].every(Number.isFinite) || headShare < .1 || headShare > .9 || topMargin < 0 || topMargin > .5 || horizontal < .1 || horizontal > .9) throw new Error('Framing controls are outside their limits.');
  const [l, t, r, b] = face.bounds;
  const h = Math.min(1, (b - t) / headShare);
  const w = Math.min(1, h * ratio / (context.width / context.height));
  const left = Math.max(0, Math.min(1 - w, (l + r) / 2 - w * horizontal));
  const top = Math.max(0, Math.min(1 - h, t - h * topMargin));
  // Preserve the requested aspect if the frame edge limits either dimension.
  const crop = ratioCrop(context.width, context.height, ratio, [left + w / 2, top + h / 2], Math.max(1, Math.min(1, ratio / (context.width / context.height)) / w));
  if (crop[0] > l + .001 || crop[1] > t + .001 || crop[2] < r - .001 || crop[3] < b - .001) throw new Error('The requested ratio and face size would cut the measured face. Use a taller ratio or smaller face share.');
  return crop;
}
export function eyeRotation(face: FramingFace, width: number, height: number): number {
  if (face.eyesMeasured < 2) throw new Error('Both eyes must be measured; estimated eye positions are not used for alignment.');
  const [a, b] = face.eyes;
  const roll = -Math.atan2((b[1] - a[1]) * height, (b[0] - a[0]) * width) * 180 / Math.PI;
  if (!Number.isFinite(roll) || Math.abs(roll) > 45) throw new Error('Eye alignment exceeds the safe rotation range.');
  return roll;
}
export function subjectRotation(axis: number): number {
  const result = axis >= 0 ? 90 - axis : -90 - axis;
  if (!Number.isFinite(result) || Math.abs(result) > 30) throw new Error('Subject silhouette has no safe upright alignment. Use manual rotation.');
  return result;
}
/** Probe displayed sRGB output, CIE Lab using the D65 reference white. Not RAW scene values. */
export function probePixel(render: RenderDto, point: [number, number]): { rgb: number[]; lab: number[] } {
  if (!point.every(v => Number.isFinite(v) && v >= 0 && v <= 1) || render.width < 1 || render.height < 1) throw new Error('Choose a point inside the displayed photo.');
  const bytes = atob(render.rgbBase64);
  if (bytes.length !== render.width * render.height * 3) throw new Error('The displayed preview has incomplete pixels.');
  const x = Math.min(render.width - 1, Math.floor(point[0] * render.width));
  const y = Math.min(render.height - 1, Math.floor(point[1] * render.height));
  const rgb = [0, 1, 2].map(c => bytes.charCodeAt((y * render.width + x) * 3 + c));
  const [r = 0, g = 0, b = 0] = rgb.map(v => { const c = v / 255; return c <= .04045 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4; });
  const xyz = [(r * .4124564 + g * .3575761 + b * .1804375) / .95047, r * .2126729 + g * .7151522 + b * .072175, (r * .0193339 + g * .119192 + b * .9503041) / 1.08883];
  const [fx = 0, fy = 0, fz = 0] = xyz.map(v => v > 216 / 24389 ? Math.cbrt(v) : (24389 / 27 * v + 16) / 116);
  return { rgb, lab: [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)] };
}
