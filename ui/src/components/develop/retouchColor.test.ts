import { expect, it } from 'vitest';
import { colorHex, colorRgb } from './retouchColor';
import { validRetouch } from './RetouchControls';
import { freshRetouch } from '../../ipc/nativeRetouch';

it('round trips user colors and rejects absent or invalid targets before saving', () => {
  for (const color of ['#000000','#ffffff','#72bd32','#ff0033']) expect(colorHex(colorRgb(color))).toBe(color);
  expect(validRetouch({...freshRetouch(),targetColor:[2,0,0]})).toBe(false);
  expect(validRetouch({...freshRetouch(),tool:'reshape',selection:{inverted:true}})).toBe(false);
  for (const tool of ['colorize','background_color'] as const) {
    const edit = {...freshRetouch(),tool};
    expect(validRetouch(edit)).toBe(false);
    expect(validRetouch({...edit,targetColor:[1,0,.3]})).toBe(true);
    expect(validRetouch({...edit,targetColor:[1,Number.NaN,0]})).toBe(false);
    expect(validRetouch({...edit,targetColor:[1,0,2]})).toBe(false);
  }
});
