import { expect, it } from 'vitest';
import { measureHistogram } from './Histogram';

it('counts channel distributions and channel clipping from real pixel bytes', () => {
  const result = measureHistogram({ width: 3, height: 1, rgbBase64: btoa(String.fromCharCode(0, 0, 0, 255, 10, 10, 128, 128, 128)) });
  expect(result.shadows).toBeCloseTo(100 / 3);
  expect(result.highlights).toBeCloseTo(100 / 3);
  expect(result.channels[0]?.[63]).toBe(1);
  expect(result.channels[1]?.[63]).toBe(0);
  expect(result.channels[2]?.[32]).toBe(1);
});

it('rejects malformed pixels rather than displaying a fabricated histogram', () => {
  expect(() => measureHistogram({ width: 2, height: 1, rgbBase64: 'AAAA' })).toThrow('Incomplete');
  expect(() => measureHistogram({ width: 0, height: 1, rgbBase64: '' })).toThrow('Invalid');
});
