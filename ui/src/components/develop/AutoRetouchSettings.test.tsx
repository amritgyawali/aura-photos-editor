import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AutoRetouchSettings, PRESETS, SETTING_GROUPS } from './AutoRetouchSettings';
import { DEFAULT_RETOUCH_SETTINGS } from '../../ipc/nativeRetouch';
import { automaticLabel } from './NativeRetouchWorkspace';

describe('automatic retouch settings', () => {
  it('offers face, body skin and face + body skin, and runs the chosen one', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    expect(screen.getByRole('radio', { name: 'Face' })).toBeTruthy();
    fireEvent.click(screen.getByRole('radio', { name: 'Body skin' }));
    expect(screen.getByText(/The face is left as it is/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Body skin' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'body' }));
    fireEvent.click(screen.getByRole('radio', { name: 'Face + body skin' }));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ scope: 'face_and_body' }));
  });
  it('sends the chosen strength and features', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByText('Strength and details'));
    fireEvent.change(screen.getByLabelText('Automatic retouch strength'), { target: { value: '60' } });
    expect(screen.getByText(/Strength: Subtle \(60%\)/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('Teeth'));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenCalledWith({ intensity: 0.6, blemishes: true, eyes: true, teeth: false, refine: true, scope: 'face', settings: DEFAULT_RETOUCH_SETTINGS, adaptive: true });
  });
  it('adapts to each face unless the photographer wants the settings used exactly', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    const adapt = screen.getByLabelText('Adapt to each face') as HTMLInputElement;
    expect(adapt.checked).toBe(true);
    expect(screen.getByText(/Each face is measured and gets its own amounts/)).toBeTruthy();
    fireEvent.click(adapt);
    expect(screen.getByText(/used exactly as set on every face/)).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenLastCalledWith(expect.objectContaining({ adaptive: false }));
  });
  it('turns face details off for body skin only and disables everything while busy', () => {
    const { rerender } = render(<AutoRetouchSettings disabled={false} onRun={vi.fn()} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Body skin' }));
    expect((screen.getByLabelText('Teeth') as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByTitle(/Dark marks are kept unless/)).toBeTruthy();
    rerender(<AutoRetouchSettings disabled busy onRun={vi.fn()} />);
    expect((screen.getByRole('button', { name: 'Detecting and retouching…' }) as HTMLButtonElement).closest('fieldset')?.disabled).toBe(true);
  });
  it('labels automatic operations by face or body', () => {
    expect(automaticLabel('auto-portrait-v1-0-texture')).toBe(' · Auto (face 1)');
    expect(automaticLabel('auto-portrait-v1-1-body-tone')).toBe(' · Auto (body 2)');
    expect(automaticLabel('auto-scene-v1-sky')).toBe(' · Auto (scene)');
    expect(automaticLabel('manual-id')).toBe('');
    expect(automaticLabel('auto-portrait-v1-0-lines-neck')).toBe(' · Auto (body 1)');
    expect(automaticLabel('auto-portrait-v1-1-hair-detail')).toBe(' · Auto (hair 2)');
    expect(automaticLabel('auto-portrait-v1-0-fabric')).toBe(' · Auto (clothes 1)');
    expect(automaticLabel('auto-portrait-v1-backdrop')).toBe(' · Auto (backdrop)');
  });
  it('offers every fine control exactly once', () => {
    const keys = SETTING_GROUPS.flatMap(([, controls]) => controls.map(([key]) => key));
    expect(keys.length).toBe(54);
    expect(new Set(keys).size).toBe(54);
    expect([...keys].sort()).toEqual(Object.keys(DEFAULT_RETOUCH_SETTINGS).sort());
  });
  it('runs deep cleanup as one native pass and exposes dark-mark removal', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Deep acne cleanup' }));
    expect(screen.getByText(/can also remove freckles or beauty marks/)).toBeTruthy();
    fireEvent.click(screen.getByText('Blemishes'));
    expect(screen.getByLabelText('Most spots per face').getAttribute('max')).toBe('220');
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    expect(run).toHaveBeenCalledWith(expect.objectContaining({ settings: expect.objectContaining({
      deepBlemishCleanup: true, removeDarkMarks: true, maxSpots: 220, keepFreckles: false,
    }) }));
  });
  it('applies a preset, then marks a hand change as custom and sends it', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Polished beauty' }));
    fireEvent.click(screen.getByText('Skin'));
    expect(screen.getByText('Skin smoothing: 85%')).toBeTruthy();
    fireEvent.change(screen.getByLabelText('Skin warmth'), { target: { value: '-40' } });
    expect(screen.getByText('Custom')).toBeTruthy();
    fireEvent.click(screen.getByText('Skin detection'));
    fireEvent.click(screen.getByLabelText('Main subject only'));
    fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
    const sent = run.mock.calls[0]?.[0];
    expect(sent?.settings).toEqual(expect.objectContaining({ smoothing: .85, contour: .5, skinWarmth: -0.4, mainSubjectOnly: true }));
    expect(PRESETS.every(([, , values]) => Object.keys(values).every(k => k in DEFAULT_RETOUCH_SETTINGS))).toBe(true);
  });
});
