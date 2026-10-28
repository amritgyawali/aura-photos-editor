import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { PhotoEditor } from './PhotoEditor';
import { photoAutoEdit } from '../../ipc/client';
import type { RecipeDto } from '../../ipc/types';

vi.mock('../../ipc/client', () => ({
  asIpcError: (e: Error) => ({ message: e.message }),
  api: { getPreview: vi.fn().mockResolvedValue({ dataUrl: 'data:image/png;base64,x' }), cancelJob: vi.fn(), listImages: vi.fn() },
  develop: {},
  photoAutoEdit: vi.fn(),
}));

const recipe = (recipeHash: string) => ({ photoId: 'p', recipeHash, body: '{}', params: [] }) as unknown as RecipeDto;

beforeEach(() => vi.mocked(photoAutoEdit).mockReset());

function mount(saved: RecipeDto) {
  render(<PhotoEditor projectId="project" photoId="p" onChanged={vi.fn().mockResolvedValue(undefined)} render={null} recipe={saved} history={null} />);
  fireEvent.click(screen.getByRole('button', { name: 'Auto edit photo' }));
}

it('says in words when Auto edit decided the photograph needed nothing', async () => {
  vi.mocked(photoAutoEdit).mockResolvedValue({ recipe: recipe('same'), source: 'cloud', model: 'm', reasons: ['Already balanced.'] });
  mount(recipe('same'));
  expect(await screen.findByText('Nothing changed: AURA judged this photograph already balanced.')).toBeTruthy();
});

it('says nothing of the kind when the photograph was edited', async () => {
  vi.mocked(photoAutoEdit).mockResolvedValue({ recipe: recipe('after'), source: 'cloud', model: 'm', reasons: ['Lifted the shadows.'] });
  mount(recipe('before'));
  expect(await screen.findByText('Lifted the shadows.')).toBeTruthy();
  expect(screen.queryByText(/Nothing changed/)).toBeNull();
});
