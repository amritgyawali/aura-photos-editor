/**
 * Pure helpers for the portrait retouch workspace: the operator list the panel offers, the
 * colour each region is drawn in, and the overlay compositor. No React and no IPC here, so every
 * line is testable without a window.
 */

/** One retouch operator, in the order the panel and the renderer apply them. */
export type PortraitOperator = {
  op: string;
  label: string;
  group: 'Skin' | 'Face' | 'Eyes' | 'Mouth' | 'Hair & background';
  hint: string;
  /** The region it works inside, so the panel can say when there is nothing to work on. */
  region: string;
};

/**
 * The fourteen operators the renderer executes (`aura_render::portrait::OPERATORS`).
 *
 * Every one changes tone or colour in place. There is deliberately no reshape, slim, enlarge or
 * skin-lightening control on this list and there never will be: those are decisions about how a
 * person looks rather than about the photograph, and `docs/retouch-ethics.md` rules them out.
 */
export const PORTRAIT_OPERATORS: ReadonlyArray<PortraitOperator> = [
  { op: 'skin_smooth', label: 'Smooth skin', group: 'Skin', hint: 'Softens blotches and uneven texture; pores stay.', region: 'skin' },
  { op: 'skin_even', label: 'Even skin tone', group: 'Skin', hint: "Evens redness toward the person's own skin tone.", region: 'skin' },
  { op: 'blemish_clear', label: 'Clear blemishes', group: 'Skin', hint: 'Heals small marks. Moles and freckles are kept.', region: 'skin' },
  { op: 'under_eye_lift', label: 'Brighten under-eyes', group: 'Skin', hint: 'Lifts shadows under the eyes toward the cheek.', region: 'under_eyes' },
  { op: 'shine_control', label: 'Reduce shine', group: 'Skin', hint: 'Softens specular shine on the face.', region: 'face' },
  { op: 'face_light', label: 'Face fill light', group: 'Face', hint: 'Lifts shadows on the face; highlights stay.', region: 'face' },
  { op: 'eye_brighten', label: 'Brighten eyes', group: 'Eyes', hint: 'A little more light in the eyes.', region: 'eyes' },
  { op: 'iris_enhance', label: 'Iris detail', group: 'Eyes', hint: 'Brings out iris texture and colour.', region: 'iris' },
  { op: 'sclera_whiten', label: 'Clear eye whites', group: 'Eyes', hint: 'Reduces redness in the whites of the eyes.', region: 'sclera' },
  { op: 'brow_define', label: 'Define brows', group: 'Eyes', hint: 'Slightly deeper, crisper eyebrows.', region: 'eyebrows' },
  { op: 'teeth_whiten', label: 'Whiten teeth', group: 'Mouth', hint: 'Takes out yellow, never to paper white.', region: 'teeth' },
  { op: 'lip_enhance', label: 'Lip colour', group: 'Mouth', hint: 'A touch more colour and shape in the lips.', region: 'lips' },
  { op: 'hair_define', label: 'Define hair', group: 'Hair & background', hint: 'Crisper strands and texture.', region: 'hair' },
  { op: 'background_blur', label: 'Blur background', group: 'Hair & background', hint: 'A shallow-focus look; the person stays sharp.', region: 'background' },
];

/** The groups, in order. */
export const OPERATOR_GROUPS = ['Skin', 'Face', 'Eyes', 'Mouth', 'Hair & background'] as const;

/** The colour each region is drawn in on the overlay. */
export const REGION_COLOURS: Readonly<Record<string, [number, number, number]>> = {
  skin: [255, 92, 170],
  face: [255, 150, 80],
  eyes: [80, 200, 255],
  iris: [0, 230, 230],
  sclera: [245, 245, 255],
  eyebrows: [150, 90, 40],
  under_eyes: [220, 80, 255],
  nose: [60, 220, 90],
  lips: [255, 40, 60],
  teeth: [255, 240, 60],
  mouth: [255, 120, 120],
  facial_hair: [255, 140, 0],
  neck: [255, 180, 200],
  hair: [230, 140, 30],
  body_skin: [255, 120, 190],
  clothing: [70, 200, 110],
  body: [120, 230, 160],
  background: [80, 90, 220],
  sky: [120, 200, 255],
};

/** Regions the overlay shows when the workspace opens: the ones people ask about first. */
export const DEFAULT_OVERLAY = ['skin', 'eyes', 'lips', 'teeth', 'hair'];

/** Decode a base64 alpha plane. Malformed input decodes to an empty plane rather than throwing. */
export function decodeAlpha(base64: string): Uint8Array {
  try {
    const text = atob(base64);
    const out = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i++) out[i] = text.charCodeAt(i);
    return out;
  } catch {
    return new Uint8Array(0);
  }
}

/**
 * Composite coloured alpha layers into one RGBA overlay. Later layers paint over earlier ones, so
 * the caller passes broad regions first and small features last.
 *
 * Accumulated premultiplied and written out **straight**, because that is what `ImageData`
 * holds: a premultiplied pink handed to a canvas is drawn as a dim mauve.
 */
export function composeOverlay(
  width: number,
  height: number,
  layers: ReadonlyArray<{ alpha: Uint8Array; colour: readonly [number, number, number] }>,
  opacity = 0.55,
): Uint8ClampedArray {
  const n = width * height;
  const premultiplied = new Float32Array(n * 4);
  for (const layer of layers) {
    if (layer.alpha.length !== n) continue;
    const [r, g, b] = layer.colour;
    for (let i = 0; i < n; i++) {
      const a = ((layer.alpha[i] ?? 0) / 255) * opacity;
      if (a <= 0) continue;
      const at = i * 4;
      const keep = 1 - a;
      premultiplied[at] = (premultiplied[at] ?? 0) * keep + r * a;
      premultiplied[at + 1] = (premultiplied[at + 1] ?? 0) * keep + g * a;
      premultiplied[at + 2] = (premultiplied[at + 2] ?? 0) * keep + b * a;
      premultiplied[at + 3] = (premultiplied[at + 3] ?? 0) * keep + a;
    }
  }
  const out = new Uint8ClampedArray(n * 4);
  for (let i = 0; i < n; i++) {
    const at = i * 4;
    const alpha = premultiplied[at + 3] ?? 0;
    if (alpha <= 0) continue;
    out[at] = (premultiplied[at] ?? 0) / alpha;
    out[at + 1] = (premultiplied[at + 1] ?? 0) / alpha;
    out[at + 2] = (premultiplied[at + 2] ?? 0) / alpha;
    out[at + 3] = alpha * 255;
  }
  return out;
}

/** A face box drawn by dragging, normalised and ordered; `null` when it is too small to be one. */
export function boxFromDrag(
  start: [number, number],
  end: [number, number],
): [number, number, number, number] | null {
  const clamp = (v: number) => Math.min(1, Math.max(0, v));
  const x0 = clamp(Math.min(start[0], end[0]));
  const y0 = clamp(Math.min(start[1], end[1]));
  const x1 = clamp(Math.max(start[0], end[0]));
  const y1 = clamp(Math.max(start[1], end[1]));
  if (x1 - x0 < 0.02 || y1 - y0 < 0.02) return null;
  return [x0, y0, x1 - x0, y1 - y0];
}

/** A strength as the percentage the slider shows. */
export function percent(strength: number): number {
  return Math.round(Math.min(1, Math.max(0, strength)) * 100);
}
