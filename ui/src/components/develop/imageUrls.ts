/**
 * Large images as blob URLs that live exactly as long as what they were made from.
 *
 * A full-quality preview is the original's own resolution (ADR-0097): 30 megapixels is a 90 MB
 * bitmap. As a `data:` URL that is a 120 MB string, built through two more copies of the same
 * size, and the browser keeps every one it has decoded until it decides to prune - measured in
 * the desktop window, each new preview added 120 MB that a forced garbage collection did not
 * return, and a slider session reached 4.4 GB and stopped responding. A blob URL hands the
 * browser the bitmap's bytes once and is released with `URL.revokeObjectURL`.
 *
 * Each URL is owned by an object - the render it shows - and released when that object is
 * collected, so a picture on screen (whose component still holds its render) is never revoked
 * under it, and a picture nobody holds any more does not outlive it. A slot holds one picture
 * per owner: an overlay redrawn at a new opacity replaces, and releases, the one before.
 *
 * Where blob URLs are unavailable (the unit tests' DOM) the same bytes become a `data:` URL.
 */

type Held = { variant: string; url: string };

const owned = new WeakMap<object, Map<string, Held>>();
const identities = new WeakMap<object, number>();
let nextIdentity = 1;

function release(url: string): void {
  if (url.startsWith('blob:')) URL.revokeObjectURL(url);
}

const registry = typeof FinalizationRegistry === 'function' ? new FinalizationRegistry<string>(release) : null;

function blobsAvailable(): boolean {
  return registry !== null && typeof Blob === 'function' && typeof URL !== 'undefined'
    && typeof URL.createObjectURL === 'function' && typeof URL.revokeObjectURL === 'function';
}

/** A stable number for an object, so a variant can name which other object it was made with. */
export function identity(value: object): number {
  let id = identities.get(value);
  if (id === undefined) {
    id = nextIdentity++;
    identities.set(value, id);
  }
  return id;
}

/** A browser-readable copy of a BMP file as a `data:` URL. */
export function bmpDataUrl(bmp: Uint8Array<ArrayBuffer>): string {
  const native = (bmp as Uint8Array & { toBase64?: () => string }).toBase64;
  if (typeof native === 'function') return `data:image/bmp;base64,${native.call(bmp)}`;
  const chunks: string[] = [];
  for (let at = 0; at < bmp.length; at += 8192) chunks.push(String.fromCharCode(...bmp.subarray(at, at + 8192)));
  return `data:image/bmp;base64,${btoa(chunks.join(''))}`;
}

/**
 * The URL of `owner`'s picture in `slot`, made by `build` (a BMP file) the first time a
 * `variant` is asked for and reused after that. A new variant releases the previous one.
 */
export function ownedImageUrl(owner: object, slot: string, variant: string,
  build: () => Uint8Array<ArrayBuffer> | null): string | null {
  let slots = owned.get(owner);
  const held = slots?.get(slot);
  if (held?.variant === variant) return held.url;
  const bmp = build();
  if (!bmp) return null;
  const url = blobsAvailable() ? URL.createObjectURL(new Blob([bmp], { type: 'image/bmp' })) : bmpDataUrl(bmp);
  if (held) release(held.url);
  if (!slots) {
    slots = new Map();
    owned.set(owner, slots);
  }
  slots.set(slot, { variant, url });
  if (url.startsWith('blob:')) registry?.register(owner, url);
  return url;
}
