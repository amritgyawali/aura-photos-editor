import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { LightroomPanel } from './LightroomPanel';
import { normaliseCurve } from './PointCurveEditor';
import type { RecipeDto } from '../../ipc/types';

const recipe = (params: { path: string; value: unknown; protected?: boolean }[]): RecipeDto => ({
  photoId: 'p', body: '{}', recipeHash: 'h', schema: 1, engine: 'aura-render/1.0.0', source: 'ai',
  params: params.map(p => ({ path: p.path, value: p.value, protected: p.protected ?? false, stage: null })),
} as unknown as RecipeDto);

const props = (overrides: Partial<Parameters<typeof LightroomPanel>[0]> = {}) => ({
  recipe: recipe([{ path: 'global.exposure', value: 0.5, protected: true }, { path: 'bw', value: null }]),
  disabled: false, aspect: 1.5, profiles: [], onSetParam: vi.fn(), onAuto: vi.fn(), onApplyProfile: vi.fn(), onSync: vi.fn(),
  ...overrides,
});

it('shows every Lightroom panel', () => {
  render(<LightroomPanel {...props()} />);
  for (const title of ['Basic', 'Tone Curve', 'Color Mixer', 'Black & White', 'Color Grading', 'Detail', 'Lens Corrections', 'Transform & Crop', 'Effects', 'Calibration']) {
    expect(screen.getByText(title, { selector: 'summary' })).toBeTruthy();
  }
});

it('commits a slider once, on release, to its recipe path', () => {
  const onSetParam = vi.fn();
  render(<LightroomPanel {...props({ onSetParam })} />);
  const [exposure] = screen.getAllByRole('slider', { hidden: true }).filter(el => (el as HTMLInputElement).max === '5');
  if (!exposure) throw new Error('no exposure slider');
  expect((exposure as HTMLInputElement).value).toBe('0.5');
  fireEvent.change(exposure, { target: { value: '1.25' } });
  expect(onSetParam).not.toHaveBeenCalled();
  fireEvent.pointerUp(exposure);
  expect(onSetParam).toHaveBeenCalledWith('global.exposure', 1.25, 'Exposure');
});

it('writes absent blocks by path and toggles black and white', () => {
  const onSetParam = vi.fn();
  render(<LightroomPanel {...props({ onSetParam })} />);
  const grain = screen.getByLabelText('Grain value');
  fireEvent.change(grain, { target: { value: '30' } });
  fireEvent.blur(grain);
  expect(onSetParam).toHaveBeenCalledWith('global.effects.grain.amount', 30, 'Grain');
  fireEvent.click(screen.getByLabelText('Convert to black & white'));
  expect(onSetParam).toHaveBeenCalledWith('bw', { mix: {}, grade: null }, 'Black & white');
});

it('crops to a centred ratio and syncs', () => {
  const onSetParam = vi.fn();
  const onSync = vi.fn();
  render(<LightroomPanel {...props({ onSetParam, onSync })} />);
  fireEvent.click(screen.getByRole('button', { name: '1:1' }));
  expect(onSetParam).toHaveBeenCalledWith('geometry.crop', [0.1667, 0, 0.8333, 1], 'Crop 1.00');
  fireEvent.click(screen.getByRole('button', { name: 'Sync settings to all photos' }));
  expect(onSync).toHaveBeenCalledWith(false);
});

it('keeps a curve valid for the recipe', () => {
  expect(normaliseCurve([[128, 140], [30, 20]])).toEqual([[0, 20], [30, 20], [128, 140], [255, 140]]);
  expect(normaliseCurve([[0, 10], [0, 30], [255, 250]])).toEqual([[0, 10], [255, 250]]);
});

it('offers essentials without hiding access to the original advanced controls', () => {
  const onAuto = vi.fn();
  const { rerender } = render(<LightroomPanel {...props({ mode: 'essentials', onAuto })} />);
  expect(screen.getByText('Light & color', { selector: 'summary' })).toBeTruthy();
  expect(screen.queryByText('Calibration', { selector: 'summary' })).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Auto enhance photo' }));
  expect(onAuto).toHaveBeenCalledOnce();
  rerender(<LightroomPanel {...props({ mode: 'advanced' })} />);
  expect(screen.getByText('Calibration', { selector: 'summary' })).toBeTruthy();
});

it('does not save empty numbers, duplicate commits, or values from disabled labels', () => {
  const onSetParam = vi.fn();
  const { rerender } = render(<LightroomPanel {...props({ onSetParam })} />);
  const input = screen.getByLabelText('Exposure value');
  fireEvent.change(input, { target: { value: '' } });
  fireEvent.blur(input);
  expect((input as HTMLInputElement).value).toBe('0.5');
  expect(onSetParam).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: '1.25' } });
  fireEvent.keyDown(input, { key: 'Enter' });
  fireEvent.blur(input);
  expect(onSetParam).toHaveBeenCalledOnce();
  expect(onSetParam).toHaveBeenCalledWith('global.exposure', 1.25, 'Exposure');
  rerender(<LightroomPanel {...props({ onSetParam, disabled: true })} />);
  fireEvent.doubleClick(screen.getByText('Exposure', { selector: 'span' }));
  expect(onSetParam).toHaveBeenCalledOnce();
});
