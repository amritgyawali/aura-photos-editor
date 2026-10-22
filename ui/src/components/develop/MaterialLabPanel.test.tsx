import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { MaterialLabPanel } from './MaterialLabPanel';
import { MATERIAL_TOOLS, materialLab } from '../../ipc/materialLab';
import type { RecipeDto } from '../../ipc/types';
vi.mock('../../ipc/materialLab', async importOriginal => ({ ...await importOriginal<typeof import('../../ipc/materialLab')>(), materialLab: { save: vi.fn() } }));
const recipe = { photoId: 'photo', recipeHash: 'current', body: JSON.stringify({ studio_masks_v1: [{ id: 'fabric', name: 'Fabric', enabled: true, amount: 1, components: [], params: {} }] }) } as RecipeDto;
it('authors all fifteen tools and saves the ordered stack with its revision', async () => {
  const onApply = vi.fn(); vi.mocked(materialLab.save).mockResolvedValue({ recipe });
  render(<MaterialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  expect(materialLab.save).not.toHaveBeenCalled();
  for (const [id] of MATERIAL_TOOLS) {
    fireEvent.change(screen.getByLabelText('Material tool'), { target: { value: id } });
    fireEvent.click(screen.getByText('Add material operation'));
  }
  fireEvent.click(screen.getByText('Apply material tools'));
  await waitFor(() => expect(onApply).toHaveBeenCalledWith(recipe));
  const call = vi.mocked(materialLab.save).mock.calls.at(-1)!;
  expect(call.slice(0, 3)).toEqual(['project', 'photo', 'current']);
  expect(call[3].map(op => op.tool)).toEqual(MATERIAL_TOOLS.map(t => t[0]));
  expect(call[3].every(op => op.maskId === 'fabric')).toBe(true);
});
it('refuses unselected tools, retains stale drafts and discards explicitly', async () => {
  const empty = { ...recipe, body: '{}' };
  const onApply = vi.fn(); const view = render(<MaterialLabPanel projectId="project" photoId="photo" recipe={empty} disabled={false} onApply={onApply}/>);
  expect((screen.getByText('Add material operation') as HTMLButtonElement).disabled).toBe(true);
  view.rerender(<MaterialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  fireEvent.change(screen.getByLabelText('Material tool'), { target: { value: 'fabric_bands' } }); fireEvent.click(screen.getByText('Add material operation'));
  fireEvent.change(screen.getByLabelText('Coarse fabric retention'), { target: { value: '0.7' } });
  fireEvent.change(screen.getByLabelText('Fine detail retention'), { target: { value: '1.3' } });
  vi.mocked(materialLab.save).mockRejectedValue({ code: 'AURA-RENDER-8002', message: 'Photo changed', runbookUrl: 'https://aura.app/e/AURA-RENDER-8002', retryable: false });
  fireEvent.click(screen.getByText('Apply material tools')); await screen.findByText('Photo changed');
  expect((screen.getByLabelText('Fine detail retention') as HTMLInputElement).value).toBe('1.3');
  fireEvent.click(screen.getByText('Discard material draft')); expect(screen.queryByLabelText('Fine detail retention')).toBeNull();
  expect(onApply).not.toHaveBeenCalled();
});
it('ignores a completed save after navigation', async () => {
  let resolve!: (value: { recipe: RecipeDto }) => void;
  vi.mocked(materialLab.save).mockReturnValue(new Promise(done => { resolve = done; }));
  const onApply = vi.fn(); const view = render(<MaterialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  fireEvent.click(screen.getByText('Add material operation')); fireEvent.click(screen.getByText('Apply material tools'));
  view.rerender(<MaterialLabPanel projectId="project" photoId="next" recipe={{ ...recipe, photoId: 'next', recipeHash: 'next' }} disabled={false} onApply={onApply}/>);
  resolve({ recipe }); await waitFor(() => expect((screen.getByText('Apply material tools') as HTMLButtonElement).disabled).toBe(false));
  expect(onApply).not.toHaveBeenCalled(); expect(screen.queryByLabelText('Selected material operation')).toBeNull();
});
it('preserves an unsaved stack when the same saved revision is reloaded', () => {
  const onApply = vi.fn(); const view = render(<MaterialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
  fireEvent.click(screen.getByText('Add material operation'));
  fireEvent.change(screen.getByLabelText('Material strength'), { target: { value: '0.8' } });
  view.rerender(<MaterialLabPanel projectId="project" photoId="photo" recipe={{ ...recipe }} disabled={false} onApply={onApply}/>);
  expect((screen.getByLabelText('Material strength') as HTMLInputElement).value).toBe('0.8');
});
