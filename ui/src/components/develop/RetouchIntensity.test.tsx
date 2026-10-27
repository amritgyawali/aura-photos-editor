import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { RetouchIntensity } from './RetouchIntensity';
import { retouchIntensity, type IntensityReport } from '../../ipc/retouchIntensity';

vi.mock('../../ipc/retouchIntensity', async () => ({
  ...await vi.importActual('../../ipc/retouchIntensity'),
  retouchIntensity: { get: vi.fn(), set: vi.fn(), reset: vi.fn(), sync: vi.fn() },
}));

const report = (smoothing = 1): IntensityReport => ({
  sliders: [
    { category: 'blemishes', operations: 14, factor: 1, headroom: 1 },
    { category: 'smoothing', operations: 2, factor: smoothing, headroom: 1.5 },
    { category: 'teeth', operations: 1, factor: 1, headroom: 2 },
  ],
  synced: 0, skipped: 0,
});

beforeEach(() => { vi.resetAllMocks(); });

const mount = (onRun = (task: () => Promise<void>) => void task(), version: string | null = 'h1') =>
  render(<RetouchIntensity projectId="p" photoId="a" version={version} disabled={false} onRun={onRun}/>);

it('shows nothing until the automatic retouch has left something to scale', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue({ sliders: [], synced: 0, skipped: 0 });
  const { container } = mount();
  await waitFor(() => expect(retouchIntensity.get).toHaveBeenCalledWith('p', 'a'));
  expect(container.textContent).toBe('');
});

it('offers one slider per kind of retouch, reaching only as high as it can change anything', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue(report());
  mount();
  const blemish = await screen.findByRole('slider', { name: 'Blemish & acne removal strength' });
  expect(blemish.getAttribute('max')).toBe('100');
  expect(screen.getByRole('slider', { name: 'Skin smoothing strength' }).getAttribute('max')).toBe('150');
  expect(screen.getByRole('slider', { name: 'Teeth whitening strength' }).getAttribute('max')).toBe('200');
  expect(screen.getByText(/14 areas/)).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Reset to Auto retouch' }) as HTMLButtonElement).disabled).toBe(true);
});

it('saves a slider once it is released, not on every step of the drag', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue(report());
  vi.mocked(retouchIntensity.set).mockResolvedValue(report(0.4));
  const onRun = vi.fn((task: () => Promise<void>) => void task());
  mount(onRun);
  const smoothing = await screen.findByRole('slider', { name: 'Skin smoothing strength' });
  fireEvent.change(smoothing, { target: { value: '60' } });
  fireEvent.change(smoothing, { target: { value: '40' } });
  expect(retouchIntensity.set).not.toHaveBeenCalled();
  expect(screen.getByText('40%')).toBeTruthy();
  fireEvent.pointerUp(smoothing);
  await waitFor(() => expect(retouchIntensity.set).toHaveBeenCalledWith('p', 'a', { smoothing: 0.4 }));
  expect(retouchIntensity.set).toHaveBeenCalledTimes(1);
  expect(onRun).toHaveBeenCalledTimes(1);
});

it('does not save a slider that was released where it started', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue(report());
  mount();
  const teeth = await screen.findByRole('slider', { name: 'Teeth whitening strength' });
  fireEvent.change(teeth, { target: { value: '100' } });
  fireEvent.pointerUp(teeth);
  fireEvent.keyUp(teeth);
  expect(retouchIntensity.set).not.toHaveBeenCalled();
});

it('resets to the automatic result and syncs the strengths to the collection', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue(report(0.4));
  vi.mocked(retouchIntensity.reset).mockResolvedValue(report());
  vi.mocked(retouchIntensity.sync).mockResolvedValue({ ...report(0.4), synced: 11, skipped: 2 });
  mount();
  const reset = await screen.findByRole('button', { name: 'Reset to Auto retouch' });
  fireEvent.click(reset);
  await waitFor(() => expect(retouchIntensity.reset).toHaveBeenCalledWith('p', 'a'));
  fireEvent.click(screen.getByRole('button', { name: 'Apply these strengths to the collection' }));
  expect(await screen.findByText('Applied to 11 other photos; 2 not auto-retouched yet were left alone.')).toBeTruthy();
});

it('re-reads the sliders whenever the saved recipe changes', async () => {
  vi.mocked(retouchIntensity.get).mockResolvedValue(report());
  const { rerender } = mount();
  await screen.findByRole('slider', { name: 'Skin smoothing strength' });
  vi.mocked(retouchIntensity.get).mockResolvedValue(report(0.25));
  rerender(<RetouchIntensity projectId="p" photoId="a" version="h2" disabled={false} onRun={task => void task()}/>);
  expect(await screen.findByText('25%')).toBeTruthy();
  expect(retouchIntensity.get).toHaveBeenCalledTimes(2);
});
