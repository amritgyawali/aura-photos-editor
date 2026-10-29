import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { defaultExportFolder, delivery } from '../../ipc/client';
import { DeliveryPanel } from './DeliveryPanel';

/** ADR-0189: reached from the automatic edit, the export runs on its own after an idle minute. */

vi.mock('../../ipc/client', () => ({
  inTauri: () => true,
  asIpcError: (error: Error) => ({ code: 'test', message: error.message }),
  api: { listImages: vi.fn().mockResolvedValue([{ id: 'a' }, { id: 'b' }, { id: 'c' }]) },
  photoCullReview: vi.fn().mockResolvedValue({ leftOut: 1, photos: [{ photoId: 'a', keep: true }, { photoId: 'b', keep: false }, { photoId: 'c', keep: true }] }),
  defaultExportFolder: vi.fn().mockResolvedValue('C:\\Users\\me\\Pictures\\AURA Exports\\Wedding 2026-10-29 103000'),
  exportWatermarked: vi.fn(),
  pickPhotoFolder: vi.fn(),
  delivery: {
    exportStatus: vi.fn().mockResolvedValue(null), exportFiles: vi.fn().mockResolvedValue([]), exportManifest: vi.fn().mockResolvedValue(null),
    deliveryStatus: vi.fn().mockResolvedValue(null), deliveryProviders: vi.fn().mockResolvedValue([]),
    exportPresets: vi.fn().mockResolvedValue([{ name: 'gallery', format: 'jpeg', quality: 90, colour: 'srgb', bitDepth: 8, resize: null, sharpen: null, naming: '{original}', sidecar: false }]),
    exportRun: vi.fn().mockResolvedValue(null), exportPreviewNames: vi.fn().mockResolvedValue([]), deliveryItems: vi.fn().mockResolvedValue([]),
  },
  learning: {
    learnStatus: vi.fn().mockResolvedValue(null), learnBuckets: vi.fn().mockResolvedValue([]), learnCompare: vi.fn().mockResolvedValue(null),
    learnConsent: vi.fn().mockResolvedValue(null), diagnosticsReport: vi.fn().mockResolvedValue(null),
  },
}));

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  vi.mocked(delivery.exportRun).mockClear();
  vi.mocked(defaultExportFolder).mockClear();
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

it('exports the finished photos into a new folder after a minute with nobody there', async () => {
  render(<DeliveryPanel projectId="p" profileId={null} onError={vi.fn()} editedPhotos={['a', 'c']} autoExport collectionName="Wedding" />);
  await screen.findByText(/AURA will export the 2 photos into a new folder in Pictures\\AURA Exports in/);
  act(() => { vi.advanceTimersByTime(59_000); });
  expect(delivery.exportRun).not.toHaveBeenCalled();
  act(() => { vi.advanceTimersByTime(1_500); });
  await waitFor(() => expect(delivery.exportRun).toHaveBeenCalledOnce());
  expect(defaultExportFolder).toHaveBeenCalledWith('Wedding');
  const job = vi.mocked(delivery.exportRun).mock.calls[0]?.[0];
  expect(job?.destination).toBe('C:\\Users\\me\\Pictures\\AURA Exports\\Wedding 2026-10-29 103000');
  expect(job?.sets[0]?.imageIds).toEqual(['a', 'c']);
  expect(job?.sets[0]?.name).toBe('gallery');
  // Once a visit: it does not export again.
  act(() => { vi.advanceTimersByTime(180_000); });
  expect(delivery.exportRun).toHaveBeenCalledOnce();
});

it('uses the folder the photographer chose, and never runs on a visit the automatic edit did not start', async () => {
  const { unmount } = render(<DeliveryPanel projectId="p" profileId={null} onError={vi.fn()} editedPhotos={['a', 'c']} />);
  await screen.findByText(/Render the 2 finished photos/);
  act(() => { vi.advanceTimersByTime(180_000); });
  expect(delivery.exportRun).not.toHaveBeenCalled();
  unmount();

  render(<DeliveryPanel projectId="p" profileId={null} onError={vi.fn()} editedPhotos={['a', 'c']} autoExport collectionName="Wedding" />);
  const folder = await screen.findByTestId('destination');
  fireEvent.change(folder, { target: { value: 'D:\\Delivered' } });
  await screen.findByText(/AURA will export the 2 photos into D:\\Delivered in/);
  act(() => { vi.advanceTimersByTime(61_000); });
  await waitFor(() => expect(delivery.exportRun).toHaveBeenCalledOnce());
  expect(vi.mocked(delivery.exportRun).mock.calls[0]?.[0].destination).toBe('D:\\Delivered');
  expect(defaultExportFolder).not.toHaveBeenCalled();
});
