import { afterEach, describe, expect, it, vi } from 'vitest';
import { identity, ownedImageUrl } from './imageUrls';

const bmp = () => new Uint8Array([66, 77, 0, 0]);

describe('owned image URLs', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('falls back to a data URL where blob URLs are unavailable', () => {
    const owner = {};
    const url = ownedImageUrl(owner, 'render', '', bmp);
    expect(url).toBe('data:image/bmp;base64,Qk0AAA==');
  });

  it('makes one blob URL per owner and variant, and releases a replaced one', () => {
    let made = 0;
    const revoked: string[] = [];
    const real = globalThis.URL;
    const fake = Object.assign(function FakeUrl() { /* never constructed */ }, real, {
      createObjectURL: () => `blob:aura/${++made}`,
      revokeObjectURL: (url: string) => { revoked.push(url); },
    });
    vi.stubGlobal('URL', fake);
    const owner = {};
    const build = vi.fn(bmp);
    const first = ownedImageUrl(owner, 'coverage', '0.5', build);
    expect(ownedImageUrl(owner, 'coverage', '0.5', build)).toBe(first);
    expect(build).toHaveBeenCalledTimes(1);
    // Another slot of the same owner is a different picture and leaves the first alone.
    const other = ownedImageUrl(owner, 'render', '', build);
    expect(other).not.toBe(first);
    // A new opacity replaces the overlay and releases the old one at once.
    const second = ownedImageUrl(owner, 'coverage', '0.8', build);
    expect(second).not.toBe(first);
    expect(revoked).toEqual([first]);
    // Nothing is made when the picture cannot be built.
    expect(ownedImageUrl({}, 'render', '', () => null)).toBeNull();
  });

  it('gives each object a stable identity', () => {
    const a = {}, b = {};
    expect(identity(a)).toBe(identity(a));
    expect(identity(a)).not.toBe(identity(b));
  });
});
