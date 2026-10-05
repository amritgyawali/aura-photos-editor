import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { useState } from 'react';
import { beforeEach, expect, it, vi } from 'vitest';
import { api } from '../../ipc/client';
import { BatchPhotoPicker } from './BatchPhotoPicker';

vi.mock('../../ipc/client', () => ({ api: { listImages: vi.fn() }, asIpcError: (e: Error) => e }));
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.listImages).mockResolvedValue([{ id: 'a', fileName: 'portrait.jpg' }, { id: 'b', fileName: 'landscape.jpg' }] as never);
});
function Picker() {
  const [selected, setSelected] = useState<string[] | null>(null);
  return <BatchPhotoPicker projectId="p" disabled={false} selected={selected} onSelect={setSelected} />;
}
async function open() {
  render(<Picker />);
  const details = screen.getByText(/Choose photos/).closest('details')!;
  details.open = true;
  fireEvent(details, new Event('toggle'));
  await screen.findByLabelText('portrait.jpg');
}
it('keeps selections while filtering and permits an empty selection', async () => {
  await open();
  fireEvent.click(screen.getByLabelText('portrait.jpg'));
  expect(screen.getByText('Choose photos · 1 selected')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Find photos'), { target: { value: 'portrait' } });
  fireEvent.click(screen.getByText('Select matching photos'));
  expect(screen.getByText('Choose photos · 2 selected')).toBeTruthy();
  fireEvent.click(screen.getByText('Clear selection'));
  expect(screen.getByText('Choose photos · 0 selected')).toBeTruthy();
  fireEvent.click(screen.getByText('Select all photos'));
  expect(screen.getByText('Choose photos · All photos')).toBeTruthy();
});
it('does not read the photo list until the picker opens', async () => {
  render(<Picker />);
  await waitFor(() => expect(api.listImages).not.toHaveBeenCalled());
});

it('bounds visible rows while allowing selection and search across the entire collection', async () => {
  const photos = Array.from({ length: 250 }, (_, i) => ({ id: String(i), fileName: i === 0 ? 'portrait.jpg' : `image-${i}.jpg` }));
  vi.mocked(api.listImages).mockResolvedValueOnce(photos.slice(0, 240) as never).mockResolvedValueOnce(photos.slice(240) as never);
  await open();
  expect(screen.getAllByRole('checkbox').length).toBe(120);
  fireEvent.click(screen.getByText('Clear selection'));
  fireEvent.click(screen.getByText('Select matching photos'));
  expect(screen.getByText(/250 selected/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: /Show more photos/ }));
  expect(screen.getAllByRole('checkbox').length).toBe(240);
  fireEvent.change(screen.getByLabelText('Find photos'), { target: { value: 'image-249' } });
  expect((screen.getByLabelText('image-249.jpg') as HTMLInputElement).checked).toBe(true);
});
