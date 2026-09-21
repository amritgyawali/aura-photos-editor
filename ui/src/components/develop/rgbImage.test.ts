import { describe, expect, it } from 'vitest';
import { rgbDataUrl } from './rgbImage';
import type { RenderDto } from '../../ipc/types';

describe('rendered RGB display', () => {
  it('encodes red and blue pixels with a correct top-down BMP header and row padding', () => {
    const source = { width: 1, height: 2, rgbBase64: btoa(String.fromCharCode(255, 0, 0, 0, 0, 255)) } as RenderDto;
    const url = rgbDataUrl(source);
    expect(url?.startsWith('data:image/bmp;base64,')).toBe(true);
    const bytes = Uint8Array.from(atob(url?.split(',')[1] ?? ''), (c) => c.charCodeAt(0));
    const header = new DataView(bytes.buffer);
    expect(header.getUint32(2, true)).toBe(62);
    expect(header.getInt32(22, true)).toBe(-2);
    expect(Array.from(bytes.slice(54))).toEqual([0, 0, 255, 0, 255, 0, 0, 0]);
  });
  it('does not display truncated pixel data as a valid image', () => {
    expect(rgbDataUrl({ width: 2, height: 2, rgbBase64: 'AAAA' } as RenderDto)).toBeNull();
  });
});
