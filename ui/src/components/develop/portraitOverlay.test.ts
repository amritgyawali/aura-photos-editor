import { describe, expect, it } from 'vitest';
import { PORTRAIT_OPERATORS, REGION_COLOURS, boxFromDrag, composeOverlay, decodeAlpha, percent } from './portraitOverlay';

describe('portrait overlay helpers', () => {
  it('composites a coloured layer at its own alpha', () => {
    const alpha = new Uint8Array([255, 0, 128, 255]);
    const rgba = composeOverlay(2, 2, [{ alpha, colour: [255, 0, 0] }], 1);
    expect(Array.from(rgba.slice(0, 4))).toEqual([255, 0, 0, 255]);
    expect(Array.from(rgba.slice(4, 8))).toEqual([0, 0, 0, 0]);
    expect(rgba[8 + 3]).toBeGreaterThan(120);
  });

  it('paints a later layer over an earlier one and skips a layer of the wrong size', () => {
    const full = new Uint8Array([255]);
    const rgba = composeOverlay(1, 1, [
      { alpha: full, colour: [0, 0, 255] },
      { alpha: full, colour: [0, 255, 0] },
      { alpha: new Uint8Array([255, 255]), colour: [255, 0, 0] },
    ], 1);
    expect(Array.from(rgba)).toEqual([0, 255, 0, 255]);
  });

  it('decodes base64 alpha and survives garbage', () => {
    expect(Array.from(decodeAlpha(btoa(String.fromCharCode(0, 7, 255))))).toEqual([0, 7, 255]);
    expect(decodeAlpha('%%%').length).toBe(0);
  });

  it('turns a drag into an ordered, clamped face box and refuses a click', () => {
    expect(boxFromDrag([0.6, 0.7], [0.2, 0.1])).toEqual([0.2, 0.1, 0.39999999999999997, 0.6]);
    expect(boxFromDrag([0.5, 0.5], [0.505, 0.51])).toBeNull();
    expect(boxFromDrag([-1, -1], [2, 2])).toEqual([0, 0, 1, 1]);
  });

  it('offers no control that reshapes a person or changes their skin tone', () => {
    const words = PORTRAIT_OPERATORS.map((op) => `${op.op} ${op.label} ${op.hint}`.toLowerCase()).join(' ');
    for (const banned of ['slim', 'reshape', 'liquify', 'enlarge', 'lighten skin', 'whiten skin', 'nose job']) {
      expect(words).not.toContain(banned);
    }
    expect(new Set(PORTRAIT_OPERATORS.map((op) => op.op)).size).toBe(14);
    for (const op of PORTRAIT_OPERATORS) expect(REGION_COLOURS[op.region]).toBeDefined();
    expect(percent(0.456)).toBe(46);
    expect(percent(4)).toBe(100);
  });
});
