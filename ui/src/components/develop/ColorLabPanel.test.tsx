import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { ColorLabPanel } from './ColorLabPanel';
import { colorLab, colorLabControlLabel, COLOR_LAB_LOCAL_TOOLS, COLOR_LAB_PIVOT_TOOLS, COLOR_LAB_SCALE_TOOLS, COLOR_LAB_TOOLS } from '../../ipc/colorLab';
import type { RecipeDto } from '../../ipc/types';
vi.mock('../../ipc/colorLab', async importOriginal => ({ ...await importOriginal<typeof import('../../ipc/colorLab')>(), colorLab: { save: vi.fn() } }));
const recipe = { photoId: 'photo', recipeHash: 'revision', body: JSON.stringify({ studio_masks_v1: [{ id: 'mask', name: 'Garment', enabled: true, amount: 1, components: [], params: {} }] }) } as RecipeDto;
it('authors existing methods within the bounded stack and saves explicit references and revision', async () => {
  const onApply = vi.fn(); vi.mocked(colorLab.save).mockResolvedValue({ recipe });
  render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  expect(COLOR_LAB_TOOLS).toHaveLength(100);
  for (const [id] of COLOR_LAB_TOOLS.slice(0,25)) {
    fireEvent.change(screen.getByLabelText('Colour method'), { target: { value: id } });
    fireEvent.click(screen.getByText('Add colour method'));
  }
  fireEvent.change(screen.getByLabelText('First reference X (full-frame fraction)'), { target: { value: '0.7' } });
  fireEvent.change(screen.getByLabelText('Target red linear'), { target: { value: '0.9' } });
  fireEvent.click(screen.getByText('Apply colour methods'));
  await waitFor(() => expect(onApply).toHaveBeenCalledWith(recipe));
  const call = vi.mocked(colorLab.save).mock.calls.at(-1)!;
  expect(call.slice(0, 3)).toEqual(['project', 'photo', 'revision']);
  expect(call[3].map(op => op.tool)).toEqual(COLOR_LAB_TOOLS.slice(0,25).map(entry => entry[0]));
  expect(call[3].at(-1)?.source[0]).toBe(0.7);
  expect(call[3].at(-1)?.target[0]).toBe(0.9);
});
it('retains unsaved draft on same-revision object refresh and save failure', async () => {
  const onApply = vi.fn();
  const view = render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  fireEvent.click(screen.getByText('Add colour method'));
  fireEvent.change(screen.getByLabelText('Colour method strength'), { target: { value: '0.8' } });
  view.rerender(<ColorLabPanel projectId="project" photoId="photo" recipe={{ ...recipe }} disabled={false} onApply={onApply}/>);
  expect((screen.getByLabelText('Colour method strength') as HTMLInputElement).value).toBe('0.8');
  vi.mocked(colorLab.save).mockRejectedValue({ message: 'Photo changed', code: 'AURA-RENDER-8002', retryable: false, runbookUrl: 'https://aura.app/e/AURA-RENDER-8002' });
  fireEvent.click(screen.getByText('Apply colour methods')); await screen.findByText('Photo changed');
  expect((screen.getByLabelText('Colour method strength') as HTMLInputElement).value).toBe('0.8');
  fireEvent.click(screen.getByText('Discard colour draft'));
  expect(screen.queryByLabelText('Selected colour operation')).toBeNull();
});
it('requires a saved mask and ignores a save finishing after photo navigation', async () => {
  const onApply = vi.fn();
  const empty = { ...recipe, body: '{}' };
  const view = render(<ColorLabPanel projectId="project" photoId="photo" recipe={empty} disabled={false} onApply={onApply}/>);
  expect((screen.getByText('Add colour method') as HTMLButtonElement).disabled).toBe(true);
  view.rerender(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  let resolve!: (value: { recipe: RecipeDto }) => void;
  vi.mocked(colorLab.save).mockReturnValue(new Promise(done => { resolve = done; }));
  fireEvent.click(screen.getByText('Add colour method')); fireEvent.click(screen.getByText('Apply colour methods'));
  view.rerender(<ColorLabPanel projectId="project" photoId="next" recipe={{ ...recipe, photoId: 'next', recipeHash: 'next' }} disabled={false} onApply={onApply}/>);
  resolve({ recipe }); await waitFor(() => expect((screen.getByText('Apply colour methods') as HTMLButtonElement).disabled).toBe(false));
  expect(onApply).not.toHaveBeenCalled();
});


it.each(COLOR_LAB_TOOLS.slice(25))('authors new native method %s with relevant controls and saves it', async (id) => {
  const onApply = vi.fn(); vi.mocked(colorLab.save).mockResolvedValue({ recipe });
  render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  fireEvent.change(screen.getByLabelText('Colour method'), { target: { value: id } });
  fireEvent.click(screen.getByText('Add colour method'));
  expect(screen.getByLabelText('Selected colour operation')).toBeTruthy();
  if (['hue_geodesic_pull', 'hue_sector_compress'].includes(id)) {
    fireEvent.change(screen.getByLabelText('Target blue linear'), { target: { value: '0.8' } });
  }
  if (['highlight_hue_restore','local_chroma_coherence','local_illumination_flatten','local_log_ratio_denoise','chroma_hue_diffusion'].includes(id)) {
    fireEvent.change(screen.getByLabelText('Colour neighborhood reach'), { target: { value: '0.08' } });
  }
  const advanced = COLOR_LAB_TOOLS.slice(75).some(entry => entry[0] === id);
  if (advanced && COLOR_LAB_LOCAL_TOOLS.includes(id)) fireEvent.change(screen.getByLabelText(colorLabControlLabel(id, 'radius', 'Colour neighborhood reach')), { target: { value: '0.12' } });
  if (advanced && COLOR_LAB_PIVOT_TOOLS.includes(id)) fireEvent.change(screen.getByLabelText(colorLabControlLabel(id, 'pivot', 'Linear luminance anchor')), { target: { value: '0.35' } });
  if (advanced && COLOR_LAB_SCALE_TOOLS.includes(id)) fireEvent.change(screen.getByLabelText(colorLabControlLabel(id, 'width', 'Colour method scale')), { target: { value: '0.45' } });
  fireEvent.click(screen.getByText('Apply colour methods'));
  await waitFor(() => expect(onApply).toHaveBeenCalledWith(recipe));
  const operations = vi.mocked(colorLab.save).mock.calls.at(-1)![3];
  if (advanced && COLOR_LAB_LOCAL_TOOLS.includes(id)) expect(operations[0]?.radius).toBe(0.12);
  if (advanced && COLOR_LAB_PIVOT_TOOLS.includes(id)) expect(operations[0]?.pivot).toBe(0.35);
  if (advanced && COLOR_LAB_SCALE_TOOLS.includes(id)) expect(operations[0]?.width).toBe(0.45);
  expect(operations).toHaveLength(1); expect(operations[0]?.tool).toBe(id);
});
it('keeps the safety limit of 32 operations when 100 methods are available', () => {
  render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
  for (let i=0;i<33;i++) fireEvent.click(screen.getByText('Add colour method'));
  expect((screen.getByText('Add colour method') as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText('Colour method stack (32/32)')).toBeTruthy();
});

it('finds analytical methods by mechanism and exposes native intensity', () => {
  render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
  fireEvent.change(screen.getByLabelText('Search colour methods'), { target: { value: 'Bradford' } });
  expect(screen.getByText('1 of 100 methods')).toBeTruthy();
  expect((screen.getByLabelText('Colour method') as HTMLSelectElement).value).toBe('bradford_adaptation');
  fireEvent.click(screen.getByText('Add colour method'));
  expect(screen.getByLabelText('Target blue linear')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Colour method intensity'), { target: { value: '0' } });
  expect((screen.getByLabelText('Colour method intensity') as HTMLInputElement).value).toBe('0');
});

it('prevents adding a previous selection when the method search has no results', () => {
  render(<ColorLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
  fireEvent.change(screen.getByLabelText('Search colour methods'), { target: { value: 'no-such-method' } });
  expect(screen.getByText('0 of 100 methods')).toBeTruthy();
  expect((screen.getByText('Add colour method') as HTMLButtonElement).disabled).toBe(true);
});
