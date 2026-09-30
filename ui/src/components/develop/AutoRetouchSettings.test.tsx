import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { AutoRetouchSettings } from './AutoRetouchSettings';

describe('automatic retouch settings', () => {
  it('sends the chosen strength and features', () => {
    const run = vi.fn();
    render(<AutoRetouchSettings disabled={false} onRun={run} />);
    fireEvent.click(screen.getByText('Automatic face and skin retouch settings'));
    fireEvent.change(screen.getByLabelText('Automatic retouch strength'), { target: { value: '60' } });
    expect(screen.getByText(/Strength: Subtle \(60%\)/)).toBeTruthy();
    fireEvent.click(screen.getByLabelText('Teeth'));
    fireEvent.click(screen.getByText('Re-run automatic retouch'));
    expect(run).toHaveBeenCalledWith({ intensity: 0.6, blemishes: true, eyes: true, teeth: false, refine: true });
  });
  it('explains that moles are kept and disables everything while busy', () => {
    render(<AutoRetouchSettings disabled onRun={vi.fn()} />);
    expect(screen.getByTitle(/Moles and freckles are always kept/)).toBeTruthy();
    expect((screen.getByText('Re-run automatic retouch') as HTMLButtonElement).disabled).toBe(true);
  });
});
