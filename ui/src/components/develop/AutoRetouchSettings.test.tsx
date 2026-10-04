import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AutoRetouchSettings } from './AutoRetouchSettings';
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
    expect(run).toHaveBeenCalledWith({ intensity: 0.6, blemishes: true, eyes: true, teeth: false, refine: true, scope: 'face' });
  });
  it('turns face details off for body skin only and disables everything while busy', () => {
    const { rerender } = render(<AutoRetouchSettings disabled={false} onRun={vi.fn()} />);
    fireEvent.click(screen.getByRole('radio', { name: 'Body skin' }));
    expect((screen.getByLabelText('Teeth') as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByTitle(/Moles and freckles are always kept/)).toBeTruthy();
    rerender(<AutoRetouchSettings disabled busy onRun={vi.fn()} />);
    expect((screen.getByRole('button', { name: 'Detecting and retouching…' }) as HTMLButtonElement).closest('fieldset')?.disabled).toBe(true);
  });
  it('labels automatic operations by face or body', () => {
    expect(automaticLabel('auto-portrait-v1-0-texture')).toBe(' · Auto (face 1)');
    expect(automaticLabel('auto-portrait-v1-1-body-tone')).toBe(' · Auto (body 2)');
    expect(automaticLabel('auto-scene-v1-sky')).toBe(' · Auto (scene)');
    expect(automaticLabel('manual-id')).toBe('');
  });
});
