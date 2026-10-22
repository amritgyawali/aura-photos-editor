import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AutomaticFinishControls } from './AutomaticFinishControls';
import { automaticStrength, type AutomaticFinish } from '../../ipc/automaticStrength';

vi.mock('../../ipc/automaticStrength', async () => ({ ...await vi.importActual('../../ipc/automaticStrength'), automaticStrength: { read: vi.fn(), apply: vi.fn() } }));
const report: AutomaticFinish = { recipeHash: 'current', strengths: { overall: 1, categories: {} },
  categories: [{ category: 'blemishes', operations: 20 }, { category: 'smoothing', operations: 2 }], newOperations: 0 };
const onApply = async (action: () => Promise<unknown>) => { await action(); };
beforeEach(() => {
  vi.clearAllMocks(); vi.mocked(automaticStrength.read).mockResolvedValue(report);
  vi.mocked(automaticStrength.apply).mockImplementation(async (_project, _photo, _hash, strengths) => ({ ...report, strengths }));
});
it('saves independent absolute strengths in one native action and permits restoration from zero', async () => {
  render(<AutomaticFinishControls projectId="p" photoId="a" revision="r" disabled={false} onApply={onApply}/>);
  await screen.findByRole('slider', { name: 'Skin smoothing' });
  fireEvent.change(screen.getByRole('slider', { name: 'Blemish removal' }), { target: { value: '0' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Skin smoothing' }), { target: { value: '150' } });
  expect(automaticStrength.apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply automatic strengths' }));
  await waitFor(() => expect(automaticStrength.apply).toHaveBeenCalledWith('p', 'a', 'current', { overall: 1, categories: { blemishes: 0, smoothing: 1.5 } }));
  fireEvent.click(screen.getByRole('button', { name: 'Restore original strengths' }));
  fireEvent.click(screen.getByRole('button', { name: 'Apply automatic strengths' }));
  await waitFor(() => expect(automaticStrength.apply).toHaveBeenLastCalledWith('p', 'a', 'current', { overall: 1, categories: {} }));
});
it('keeps controls from a previous photo from appearing after navigation', async () => {
  let finish!: (value: AutomaticFinish) => void;
  vi.mocked(automaticStrength.read).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const view = render(<AutomaticFinishControls projectId="p" photoId="a" revision="r" disabled={false} onApply={onApply}/>);
  view.rerender(<AutomaticFinishControls projectId="p" photoId="b" revision="r" disabled={false} onApply={onApply}/>);
  await screen.findByRole('slider', { name: 'Skin smoothing' });
  await act(async () => finish({ ...report, strengths: { overall: 0, categories: {} } }));
  expect((screen.getByRole('slider', { name: 'Overall automatic retouch' }) as HTMLInputElement).value).toBe('100');
});
it('reports native refusal and allows a fresh control read', async () => {
  vi.mocked(automaticStrength.apply).mockRejectedValue(new Error('This photo changed. Reload finishing controls before applying.'));
  render(<AutomaticFinishControls projectId="p" photoId="a" revision="r" disabled={false} onApply={onApply}/>);
  await screen.findByRole('slider', { name: 'Skin smoothing' });
  fireEvent.click(screen.getByRole('button', { name: 'Apply automatic strengths' }));
  await screen.findByRole('alert');
  fireEvent.click(screen.getByRole('button', { name: 'Reload finishing controls' }));
  await waitFor(() => expect(automaticStrength.read).toHaveBeenCalledTimes(2));
});
