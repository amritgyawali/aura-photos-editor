import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { EffectControls } from './EffectControls';
import { EFFECT_COLOR, EFFECT_DEFAULTS, EFFECT_PARAMS, EFFECT_SHORTCUTS, EFFECT_TOOLS, type EffectTool } from '../../ipc/retouchEffects';
import { freshRetouch, needsRetouchSource, RETOUCH_TOOLS, type NativeRetouchEdit } from '../../ipc/nativeRetouch';
import { validRetouch } from './RetouchControls';

// The Rust enum the renderer reads, as text: every id the window can send must be a variant.
const rust = Object.values(import.meta.glob('../../../../crates/aura-recipe/src/retouch_tools.rs', { query: '?raw', import: 'default', eager: true }))[0] as string;
const snake = (name: string) => name.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();
const variants = new Set((/pub enum Tool \{([\s\S]*?)\n\}/.exec(rust)?.[1] ?? '')
  .split('\n').map(line => line.trim()).filter(line => /^[A-Z][A-Za-z]*,$/.test(line)).map(line => snake(line.slice(0, -1))));

const draft = (tool: EffectTool): NativeRetouchEdit & { tool: EffectTool } => ({ ...freshRetouch(), ...EFFECT_DEFAULTS[tool], tool, targetColor: EFFECT_COLOR[tool]?.initial ?? null });

it('names only tools the renderer knows, each once, all offered in the tool list', () => {
  expect(variants.size).toBeGreaterThan(40);
  const ids = EFFECT_TOOLS.map(t => t[0]);
  expect(new Set(ids).size).toBe(48);
  for (const id of ids) {
    expect(variants.has(id), id).toBe(true);
    expect(RETOUCH_TOOLS.some(t => t[0] === id)).toBe(true);
  }
});

it('starts every effect at a valid operation, and asks for a point only where one is needed', () => {
  for (const [id] of EFFECT_TOOLS) {
    const d = draft(id);
    const withPoint = { ...d, source: [.5, .5] as [number, number] };
    expect(validRetouch(withPoint), id).toBe(true);
    expect(validRetouch(d), id).toBe(!needsRetouchSource(d));
  }
  expect(needsRetouchSource(draft('chroma_key'))).toBe(true);
  expect(needsRetouchSource(draft('local_white_balance'))).toBe(true);
  expect(needsRetouchSource(draft('spin_blur'))).toBe(false);
});

it('refuses an inverted selection for a warp, as for local proportions', () => {
  const d = { ...draft('liquify_bloat'), selection: { inverted: true } };
  expect(validRetouch(d)).toBe(false);
  expect(validRetouch({ ...draft('haze'), selection: { inverted: true } })).toBe(true);
});

it('keeps every slider inside the range the recipe accepts', () => {
  const ranges = { tone: [0, 1], warmth: [-1, 1], tint: [-1, 1], texture: [0, 2], radius: [.0005, .05] } as const;
  for (const [id] of EFFECT_TOOLS) {
    for (const p of EFFECT_PARAMS[id]) {
      const [lo, hi] = ranges[p.key];
      expect(p.min, `${id}.${p.key}`).toBeGreaterThanOrEqual(lo);
      expect(p.max, `${id}.${p.key}`).toBeLessThanOrEqual(hi);
    }
  }
});

it('shows a tool its own sliders and writes what they say', () => {
  const onChange = vi.fn();
  render(<EffectControls draft={draft('drop_shadow')} onChange={onChange} onTool={vi.fn()}/>);
  fireEvent.change(screen.getByRole('slider', { name: 'Darkness' }), { target: { value: '0.9' } });
  expect(onChange).toHaveBeenCalledWith({ tone: .9 });
  fireEvent.change(screen.getByRole('slider', { name: 'Shadow right (+) or left (−)' }), { target: { value: '-0.5' } });
  expect(onChange).toHaveBeenCalledWith({ warmth: -.5 });
});

it('offers a toggle, an optional colour and a source hint where a tool has them', () => {
  const onChange = vi.fn();
  const { rerender } = render(<EffectControls draft={draft('weather')} onChange={onChange} onTool={vi.fn()}/>);
  fireEvent.click(screen.getByRole('checkbox', { name: 'Rain instead of snow' }));
  expect(onChange).toHaveBeenCalledWith({ tint: 1 });
  rerender(<EffectControls draft={draft('chroma_key')} onChange={onChange} onTool={vi.fn()}/>);
  expect(screen.getByText(/Pick the screen colour on the photo — needed before applying/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Only remove the spill' }));
  expect(onChange).toHaveBeenCalledWith({ targetColor: null });
});

it('a shortcut chooses its tool and then sets it up', () => {
  const onChange = vi.fn();
  const onTool = vi.fn();
  render(<EffectControls draft={draft('haze')} onChange={onChange} onTool={onTool}/>);
  fireEvent.click(screen.getByText('Shortcuts'));
  fireEvent.click(screen.getByRole('button', { name: 'Enlarge eyes' }));
  const shortcut = EFFECT_SHORTCUTS.find(s => s.label === 'Enlarge eyes');
  expect(onTool).toHaveBeenCalledWith('liquify_bloat');
  expect(onChange).toHaveBeenCalledWith(shortcut?.patch);
});
