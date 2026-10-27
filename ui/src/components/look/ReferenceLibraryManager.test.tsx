import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { ReferenceLibraryManager } from './ReferenceLibraryManager';
import { referenceLibraryWorkflow, type LibrarySnapshot } from '../../ipc/referenceLibraryWorkflows';

vi.mock('../../ipc/client', () => ({ inTauri: () => true, asIpcError: (error: Error) => ({ message: error.message }) }));
vi.mock('../../ipc/referenceLibraryWorkflows', () => ({ referenceLibraryWorkflow: vi.fn() }));
const warm = { id: 'a', name: 'Warm', width: 32, height: 32, convertedIcc: false, regions: ['whole image'], group: '' };
const cool = { ...warm, id: 'b', name: 'Cool' };
const baseline: LibrarySnapshot = { groups: ['', 'Wedding', 'Empty'], references: [warm, cool], revision: 'r1', document: null, changed: false };
const controls = { amount: 0.8, tone: 1.2, color: 0.6, whiteBalance: 0.7 };
const updated = vi.fn();
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(referenceLibraryWorkflow).mockResolvedValue(baseline);
});
async function open() {
  render(<ReferenceLibraryManager disabled={false} controls={controls} onUpdated={updated}/>);
  fireEvent.click(screen.getByText('Organize groups and selected references'));
  await screen.findByRole('checkbox', { name: 'Select reference Warm' });
}

it('submits one revision-bound request for a multiselection and saves current fine controls only explicitly', async () => {
  await open();
  fireEvent.click(screen.getByRole('checkbox', { name: 'Select reference Warm' }));
  fireEvent.click(screen.getByRole('checkbox', { name: 'Select reference Cool' }));
  fireEvent.change(screen.getByRole('combobox', { name: 'Destination reference group' }), { target: { value: 'Wedding' } });
  expect(referenceLibraryWorkflow).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole('button', { name: 'Move selected references' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenCalledWith({ action: 'move', expectedRevision: 'r1', group: 'Wedding', referenceIds: ['a', 'b'] }));
  await screen.findByRole('status');
  fireEvent.click(screen.getByRole('button', { name: 'Save current matching strengths for selected references' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenLastCalledWith({ action: 'defaults', expectedRevision: 'r1', referenceIds: ['a', 'b'], controls }));
});

it('creates a genuinely empty group, keeps Ungrouped fixed, and offers group-safe deletion', async () => {
  await open();
  expect((screen.getByRole('button', { name: 'Delete group and keep references' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('New group name'), { target: { value: 'Ceremony' } });
  vi.mocked(referenceLibraryWorkflow).mockResolvedValueOnce({ ...baseline, groups: [...baseline.groups, 'Ceremony'], revision: 'r2', changed: true });
  fireEvent.click(screen.getByRole('button', { name: 'Create empty reference group' }));
  await screen.findByRole('status');
  fireEvent.change(screen.getByRole('combobox', { name: 'Manage reference group' }), { target: { value: 'Ceremony' } });
  expect(screen.getByText('This group is empty.')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Delete group and keep references' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenLastCalledWith({ action: 'delete_group', expectedRevision: 'r2', group: 'Ceremony' }));
});

it('submits complete explicit group/reference orders without moving Ungrouped', async () => {
  await open();
  fireEvent.click(screen.getByRole('button', { name: 'Move reference Warm down' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenLastCalledWith({ action: 'reorder_references', expectedRevision: 'r1', group: '', order: ['b', 'a'] }));
  await screen.findByRole('status');
  fireEvent.change(screen.getByRole('combobox', { name: 'Manage reference group' }), { target: { value: 'Empty' } });
  fireEvent.click(screen.getByRole('button', { name: 'Move group up' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenLastCalledWith({ action: 'reorder_groups', expectedRevision: 'r1', order: ['Empty', 'Wedding'] }));
});

it('exports descriptor data and imports only after explicit action and replacement choice', async () => {
  await open();
  fireEvent.click(screen.getByText('Portable AURA reference library'));
  vi.mocked(referenceLibraryWorkflow).mockResolvedValueOnce({ ...baseline, document: '{"version":1,"references":[]}' });
  fireEvent.click(screen.getByRole('button', { name: 'Export entire descriptor library' }));
  await screen.findByText(/Descriptor library ready/);
  expect((screen.getByRole('textbox', { name: 'Portable library document' }) as HTMLTextAreaElement).value).toContain('"version":1');
  expect(referenceLibraryWorkflow).toHaveBeenCalledTimes(2);
  fireEvent.click(screen.getByRole('checkbox', { name: 'Update metadata for identical existing descriptors' }));
  fireEvent.click(screen.getByRole('button', { name: 'Import descriptor library' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenLastCalledWith({ action: 'import', expectedRevision: 'r1', document: '{"version":1,"references":[]}', replaceExisting: true }));
});

it('surfaces stale/native errors and ignores late updates after leaving the component', async () => {
  const view = render(<ReferenceLibraryManager disabled={false} onUpdated={updated}/>);
  fireEvent.click(screen.getByText('Organize groups and selected references'));
  await screen.findByRole('checkbox', { name: 'Select reference Warm' });
  vi.mocked(referenceLibraryWorkflow).mockRejectedValueOnce(new Error('The reference library changed. Refresh it.'));
  fireEvent.change(screen.getByLabelText('New group name'), { target: { value: 'Stale' } });
  fireEvent.click(screen.getByRole('button', { name: 'Create empty reference group' }));
  await screen.findByRole('alert');
  expect(updated).toHaveBeenCalledTimes(1);
  let finish!: (value: LibrarySnapshot) => void;
  vi.mocked(referenceLibraryWorkflow).mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  fireEvent.click(screen.getByRole('button', { name: 'Refresh organized library' }));
  await waitFor(() => expect(referenceLibraryWorkflow).toHaveBeenCalledTimes(3));
  view.unmount();
  await act(async () => finish({ ...baseline, references: [] }));
  expect(updated).toHaveBeenCalledTimes(1);
});
