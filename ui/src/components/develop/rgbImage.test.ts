import { describe, expect, it } from 'vitest';
import { changesDataUrl, coverageDataUrl, rgbDataUrl } from './rgbImage';
import type { RenderDto } from '../../ipc/types';

describe('rendered RGB display', () => {
  it('tints only selected pixels and rejects mismatched coverage',()=>{
    const photo={width:2,height:1,rgbBase64:btoa(String.fromCharCode(90,100,110,90,100,110))};
    const mask={...photo,rgbBase64:btoa(String.fromCharCode(0,0,0,255,255,255))};
    const url=coverageDataUrl(photo,mask,.5);
    const bytes=Uint8Array.from(atob(url?.split(',')[1]??''),c=>c.charCodeAt(0));
    expect(Array.from(bytes.slice(54,60))).toEqual([110,100,90,150,160,65]);
    expect(coverageDataUrl(photo,{...mask,width:1},.5)).toBeNull();
  });
  it('tints only the pixels the retouch changed',()=>{
    const before={width:2,height:1,rgbBase64:btoa(String.fromCharCode(90,100,110,90,100,110))};
    const after={...before,rgbBase64:btoa(String.fromCharCode(90,100,110,120,100,110))};
    const url=changesDataUrl(before,after,1);
    const bytes=Uint8Array.from(atob(url?.split(',')[1]??''),c=>c.charCodeAt(0));
    // BMP stores BGR: the unchanged pixel keeps its bytes, the changed one is fully tinted.
    expect(Array.from(bytes.slice(54,60))).toEqual([110,100,90,40,120,255]);
    expect(changesDataUrl(before,{...after,height:2},1)).toBeNull();
  });
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
    expect(rgbDataUrl({ width: 2, height: 2, rgbBase64: 'not base64!' } as RenderDto)).toBeNull();
  });
});
