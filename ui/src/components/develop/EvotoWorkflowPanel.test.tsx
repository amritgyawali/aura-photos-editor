import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

const calls = vi.hoisted(() => ({ frame: vi.fn(), export: vi.fn(), blinkCheck: vi.fn(), faces: vi.fn() }));
vi.mock('../../ipc/evotoWorkflow', async importOriginal => {
  const actual = await importOriginal<typeof import('../../ipc/evotoWorkflow')>();
  return { ...actual, evotoWorkflow: calls };
});
vi.mock('../../ipc/client', async importOriginal => {
  const actual = await importOriginal<typeof import('../../ipc/client')>();
  return { ...actual, pickPhotoFolder: vi.fn().mockResolvedValue('F:/out') };
});

import { EvotoWorkflowPanel } from './EvotoWorkflowPanel';

afterEach(cleanup);
beforeEach(() => { Object.values(calls).forEach(fn => fn.mockReset()); });

const write = (action: () => Promise<unknown>) => action().then(() => undefined);

it('a framing preset is saved through the editor and its reasons are shown', async () => {
  calls.frame.mockResolvedValue({ crop: [0.2, 0.1, 0.8, 0.9], rotate: -1.2, whiteBackground: true, reasons: ['The eye line was levelled by 1.2 degrees.'], recipe: {} });
  const onWrite = vi.fn(write);
  render(<EvotoWorkflowPanel projectId="p" photoId="a" disabled={false} onWrite={onWrite} />);
  fireEvent.click(screen.getByRole('button', { name: 'Passport 35×45 mm' }));
  expect(await screen.findByText('The eye line was levelled by 1.2 degrees.')).toBeTruthy();
  expect(calls.frame).toHaveBeenCalledWith('p', 'a', 'id_35x45');
  expect(onWrite).toHaveBeenCalledTimes(1);
});

it('a failed framing write is not described as saved', async () => {
  render(<EvotoWorkflowPanel projectId="p" photoId="a" disabled={false} onWrite={async () => undefined} />);
  fireEvent.click(screen.getByRole('button', { name: 'Headshot 4:5' }));
  await waitFor(() => expect(screen.queryByText('Working...')).toBeNull());
  expect(screen.queryByText(/Framing saved/)).toBeNull();
});

it('exports need a folder, and pass the grid rows and carousel slides', async () => {
  calls.export.mockResolvedValue({ files: ['F:/out/a-grid-post01-r2c3.jpg'], width: 1000, height: 1000, reasons: ['Post the files in the order of their post number: post01 first.'] });
  render(<EvotoWorkflowPanel projectId="p" photoId="a" disabled={false} onWrite={write} />);
  const grid = screen.getByRole('button', { name: 'Instagram grid' }) as HTMLButtonElement;
  expect(grid.disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Choose...' }));
  await waitFor(() => expect((screen.getByLabelText('Export folder') as HTMLInputElement).value).toBe('F:/out'));
  fireEvent.change(screen.getByLabelText('Grid rows'), { target: { value: '2' } });
  fireEvent.click(screen.getByRole('button', { name: 'Instagram grid' }));
  expect(await screen.findByText(/post01 first/)).toBeTruthy();
  expect(calls.export).toHaveBeenCalledWith('p', 'a', 'grid', 'F:/out', 2);
  fireEvent.change(screen.getByLabelText('Carousel slides'), { target: { value: '4' } });
  calls.export.mockResolvedValue({ files: [], width: 1, height: 1, reasons: [] });
  fireEvent.click(screen.getByRole('button', { name: 'Carousel slides' }));
  await waitFor(() => expect(calls.export).toHaveBeenLastCalledWith('p', 'a', 'carousel', 'F:/out', 4));
});

it('the eye check lists the photographs showing no white of the eye', async () => {
  calls.blinkCheck.mockResolvedValue({ checked: 27, withoutFaces: 2, flagged: [{ photoId: 'abcdef123456', fileName: 'IMG_0001.CR3', faces: [{}, {}], closed: 1, unchecked: 0 }] });
  const onOpenPhoto = vi.fn();
  render(<EvotoWorkflowPanel projectId="p" photoId="a" disabled={false} onWrite={write} onOpenPhoto={onOpenPhoto} />);
  fireEvent.click(screen.getByRole('button', { name: 'Find faces showing no white of the eye' }));
  expect(await screen.findByText('Checked 27 photographs: 1 to look at, 2 without a face.')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'IMG_0001.CR3: 1 of 2 faces showing no white of the eye' }));
  expect(onOpenPhoto).toHaveBeenCalledWith('abcdef123456');
});
