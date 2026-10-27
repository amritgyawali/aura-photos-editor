import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { App } from './App';
import { useStore } from './state/store';
import { api } from './ipc/client';

const state = vi.hoisted(() => ({ imported: false, ingest: null as null | ((event: unknown) => void) }));
vi.mock('./ipc/client', async () => ({
  ...await vi.importActual('./ipc/client'),
  inTauri: () => true,
  pickPhotos: vi.fn(async () => ['C:/photos/portrait.jpg']),
  licence: { status: vi.fn(async () => null) },
  editProfiles: { list: vi.fn(async () => []) },
  api: {
    listProjects: vi.fn(async () => [{ id: 'project', name: 'Portraits', photoCount: 1 }]),
    listImages: vi.fn(async () => state.imported ? [{ id: 'portrait', fileName: 'portrait.jpg' }] : []),
    listProblems: vi.fn(async () => []),
    onPreviewEvent: vi.fn(async () => () => undefined),
    onIngestEvent: vi.fn(async (callback: (event: unknown) => void) => { state.ingest = callback; return () => undefined; }),
    startIngest: vi.fn(async () => ({ jobId: 'import' })),
    cancelJob: vi.fn(async () => undefined),
    getPreview: vi.fn(async () => ({ dataUrl: 'data:image/png;base64,AA', width: 1, height: 1, tier: 0 })),
  },
}));
vi.mock('./components/workflow/AutomaticProgress', () => ({ AutomaticProgress: () => null }));
vi.mock('./components/workflow/FinishFolder', () => ({ FinishFolder: () => null }));
vi.mock('./components/profiles/ProfileGallery', () => ({ ProfileGallery: () => null }));
vi.mock('./components/look/InstagramStyle', () => ({ InstagramStyle: () => null }));
vi.mock('./components/autopilot/AutopilotPanel', () => ({ AutopilotPanel: ({ automaticRequest }: { automaticRequest: number }) => <span data-testid="auto-request">{automaticRequest}</span> }));
vi.mock('./components/develop/PhotoStudio', () => ({ PhotoStudio: ({ featureMode, referenceMode }: { featureMode: string | null; referenceMode: boolean }) => <section aria-label="Selected photo tool">{referenceMode ? 'color' : featureMode}</section> }));
vi.mock('./components/style/StylePanel', () => ({ StylePanel: () => <section aria-label="Look training">Train your profile</section> }));

afterEach(() => {
  cleanup();
  useStore.setState({ activeProjectId: null, rows: [], projects: [], lastError: null, progress: { done: 0, total: 0, running: false, jobId: null } });
  state.imported = false; state.ingest = null;
});

it('imports without cancelling or auto editing, then opens each tool on the imported photograph', async () => {
  useStore.setState({ activeProjectId: 'project', rows: [], focusedIndex: 0 });
  render(<App />);
  fireEvent.click(within(screen.getByRole('navigation', { name: 'Workspace' })).getByRole('button', { name: /Hot Features/ }));
  await waitFor(() => expect(state.ingest).not.toBeNull());
  fireEvent.click(screen.getByRole('button', { name: 'Add photos for Hot Features' }));
  await waitFor(() => expect(api.startIngest).toHaveBeenCalledWith({ projectId: 'project', roots: ['C:/photos/portrait.jpg'] }));
  await waitFor(() => expect(useStore.getState().progress.jobId).toBe('import'));
  expect(api.cancelJob).not.toHaveBeenCalled();
  state.imported = true;
  await act(async () => { state.ingest?.({ kind: 'finished', inserted: 1 }); });
  await waitFor(() => expect(screen.getByRole('region', { name: 'Selected photo tool' }).textContent).toBe('liquify'));
  expect(screen.getByRole('heading', { name: 'Hot Features' })).toBeTruthy();
  expect(screen.getByTestId('auto-request').textContent).toBe('0');
  for (const [title, id] of [['Face Freckles & Acne', 'skin'], ['Remove Glasses Glare', 'glare'], ['AI Color Match', 'color']]) {
    fireEvent.click(screen.getByRole('button', { name: title }));
    expect(screen.getByRole('region', { name: 'Selected photo tool' }).textContent).toBe(id);
  }
  fireEvent.click(screen.getByRole('button', { name: 'Train Your AI Look' }));
  expect(screen.getByRole('region', { name: 'Look training' })).toBeTruthy();
  expect(screen.queryByRole('region', { name: 'Selected photo tool' })).toBeNull();
});
