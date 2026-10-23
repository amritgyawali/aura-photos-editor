import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { StudioColorPanel } from './StudioColorPanel';
import { colorOf, studioColor } from '../../ipc/studioColor';
import type { RecipeDto } from '../../ipc/types';
vi.mock('../../ipc/studioColor', async importOriginal => ({ ...await importOriginal<typeof import('../../ipc/studioColor')>(), studioColor: { save: vi.fn(), measure: vi.fn() } }));
const recipe = { photoId: 'photo', body: JSON.stringify({ studio_masks_v1: [{ id: 'left', name: 'Left side', params: {}, components: [], amount: 1, enabled: true }] }), recipeHash: 'current' } as RecipeDto;
function panel() { const onApply = vi.fn(); render(<StudioColorPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply} />); return onApply; }

it('does no work on mount and saves channel Levels and selective CMYK as a single draft', async () => {
  vi.mocked(studioColor.save).mockClear(); vi.mocked(studioColor.measure).mockClear(); const onApply = panel();
  expect(studioColor.save).not.toHaveBeenCalled(); expect(studioColor.measure).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText('Levels channel'), { target: { value: 'red' } });
  fireEvent.change(screen.getByLabelText('Midtone gamma'), { target: { value: '1.4' } });
  fireEvent.change(screen.getByLabelText('Selective cyan'), { target: { value: '30' } });
  vi.mocked(studioColor.save).mockResolvedValue({ color: colorOf(recipe), recipe });
  fireEvent.click(screen.getByText('Apply colour tools'));
  await waitFor(() => expect(onApply).toHaveBeenCalledWith(recipe));
  const args = vi.mocked(studioColor.save).mock.calls.at(-1)!;
  expect(args.slice(0, 3)).toEqual(['project', 'photo', 'current']); expect(args[3].levels.red.gamma).toBe(1.4); expect(args[3].selective.ranges.reds?.cyan).toBe(30);
});
it('proposes measured points without losing unrelated selective drafts or writing a recipe', async () => {
  vi.mocked(studioColor.save).mockClear(); const measured = colorOf(recipe); measured.levels.rgb.inputBlack = 0.1;
  vi.mocked(studioColor.measure).mockResolvedValue({ color: measured, recipeHash: 'current', description: 'Measured patch' }); panel();
  fireEvent.change(screen.getByLabelText('Selective yellow'), { target: { value: '25' } });
  fireEvent.click(screen.getByText('Sample input black point'));
  await screen.findByText('Measured patch. Review and apply.');
  expect(studioColor.measure).toHaveBeenLastCalledWith('project', 'photo', 'current', 'black', 'rgb', 0.5, 0.5);
  expect((screen.getByLabelText('Input black point') as HTMLInputElement).value).toBe('0.1');
  expect((screen.getByLabelText('Selective yellow') as HTMLInputElement).value).toBe('25'); expect(studioColor.save).not.toHaveBeenCalled();
});
it('exposes real point curves, eight HSL bands, wheels and spatial controls for one existing mask', async () => {
  vi.mocked(studioColor.save).mockClear(); panel();
  fireEvent.click(screen.getByText('Extended local colour tools'));
  fireEvent.click(screen.getByText('Add local curve point'));
  fireEvent.change(screen.getByLabelText('Local curve output 2'), { target: { value: '180' } });
  fireEvent.change(screen.getByLabelText('Local HSL band'), { target: { value: 'orange' } });
  fireEvent.change(screen.getByLabelText('Local HSL s'), { target: { value: '-30' } });
  fireEvent.change(screen.getByLabelText('Local grading hue'), { target: { value: '240' } });
  fireEvent.change(screen.getByLabelText('Local grading saturation'), { target: { value: '20' } });
  fireEvent.change(screen.getByLabelText('Local sharpen'), { target: { value: '65' } });
  fireEvent.change(screen.getByLabelText('Local dehaze'), { target: { value: '20' } });
  fireEvent.change(screen.getByLabelText('Local vibrance'), { target: { value: '40' } });
  vi.mocked(studioColor.save).mockResolvedValue({ color: colorOf(recipe), recipe }); fireEvent.click(screen.getByText('Apply colour tools'));
  await waitFor(() => expect(studioColor.save).toHaveBeenCalled());
  const local = vi.mocked(studioColor.save).mock.calls.at(-1)![3].local.left!;
  expect(local.curve.points).toEqual([[0, 0], [128, 180], [255, 255]]); expect(local.hsl.orange?.s).toBe(-30);
  expect(local.grade.shadows).toEqual({ hue: 240, saturation: 20, luminance: 0 }); expect([local.sharpen, local.dehaze, local.vibrance]).toEqual([65, 20, 40]);
});
it('ignores a measurement arriving after photo navigation', async () => {
  let resolve!: (value: Awaited<ReturnType<typeof studioColor.measure>>) => void;
  vi.mocked(studioColor.measure).mockReturnValue(new Promise(done => { resolve = done; }));
  const onApply = vi.fn(); const view = render(<StudioColorPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply} />);
  fireEvent.click(screen.getByText('Measure percentile auto Levels'));
  view.rerender(<StudioColorPanel projectId="project" photoId="next" recipe={{ ...recipe, recipeHash: 'next' }} disabled={false} onApply={onApply} />);
  const color = colorOf(recipe); color.levels.rgb.inputBlack = 0.4; resolve({ color, recipeHash: 'current', description: 'Old result' });
  await waitFor(() => expect((screen.getByLabelText('Input black point') as HTMLInputElement).value).toBe('0'));
  expect(screen.queryByText('Old result. Review and apply.')).toBeNull();
});
it('shows the backend error and leaves the draft available after a stale save', async () => {
  const onApply = panel(); vi.mocked(studioColor.save).mockRejectedValue({ message: 'Photo changed. Reload colour controls.', code: 'AURA-RENDER-8002', runbookUrl: 'https://aura.app/e/AURA-RENDER-8002', retryable: false });
  fireEvent.change(screen.getByLabelText('Midtone gamma'), { target: { value: '1.6' } });
  fireEvent.click(screen.getByText('Apply colour tools'));
  await screen.findByText('Photo changed. Reload colour controls.');
  expect(onApply).not.toHaveBeenCalled(); expect((screen.getByLabelText('Midtone gamma') as HTMLInputElement).value).toBe('1.6');
});
it('selects the measured HSL band without replacing draft colour settings', async () => {
  vi.mocked(studioColor.measure).mockResolvedValue({ color: colorOf(recipe), recipeHash: 'current', description: 'Selected orange', hslBand: 'orange' }); panel();
  fireEvent.change(screen.getByLabelText('Selective cyan'), { target: { value: '18' } });
  fireEvent.click(screen.getByText('Extended local colour tools'));
  fireEvent.click(screen.getByText('Select HSL band from sample'));
  await screen.findByText('Selected orange. Review and apply.');
  expect((screen.getByLabelText('Local HSL band') as HTMLSelectElement).value).toBe('orange');
  expect((screen.getByLabelText('Selective cyan') as HTMLInputElement).value).toBe('18');
});
