import { expect, it } from 'vitest';
import { clippingPreview, imagePoint } from './previewTools';
import type { RenderDto } from '../../ipc/types';

it('marks clipped channels and shadows without altering the source RGB', () => {
  const rgb = btoa(String.fromCharCode(70, 80, 90, 70, 80, 90, 0, 1, 2, 253, 80, 90));
  const source = { width: 4, height: 1, rgbBase64: rgb } as RenderDto;
  const url = clippingPreview(source, 'both')!;
  const bytes = Uint8Array.from(atob(url.split(',')[1] ?? ''), c => c.charCodeAt(0));
  expect([...bytes.slice(54)]).toEqual([90, 80, 70, 90, 80, 70, 255, 112, 32, 48, 32, 255]);
  expect(source.rgbBase64).toBe(rgb);
  expect(clippingPreview({ ...source, rgbBase64: 'bad!' }, 'both')).toBeNull();
});

it('maps portrait and landscape letterboxes back to uncropped original coordinates', () => {
  expect(imagePoint(150, 100, 300, 200, 1)).toEqual([0.5, 0.5]);
  expect(imagePoint(20, 100, 300, 200, 1)).toBeNull();
  expect(imagePoint(100, 25, 200, 200, 2)).toBeNull();
  expect(imagePoint(200, 150, 200, 200, 2)).toEqual([1, 1]);
  expect(imagePoint(0, 0, 0, 200, 1)).toBeNull();
});
