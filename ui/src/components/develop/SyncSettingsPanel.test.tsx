import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { SyncSettingsPanel, SYNC_GROUPS } from './SyncSettingsPanel';
import { useStore } from '../../state/store';

it('never converts an empty selected subset into a whole-collection sync', () => {
  useStore.setState({ activeProjectId: 'p', selection: new Set(['source']) });
  const onSync = vi.fn();
  const view = render(<SyncSettingsPanel projectId="p" photoId="source" disabled={false} onSync={onSync} />);
  fireEvent.click(screen.getByText('Synchronize settings'));
  fireEvent.change(screen.getByLabelText('Copy to'), { target: { value: 'selected' } });
  fireEvent.click(screen.getByText('Apply selected settings'));
  expect(onSync).not.toHaveBeenCalled();
  view.unmount();
  useStore.setState({ activeProjectId: 'p', selection: new Set(['source', 'target']) });
  render(<SyncSettingsPanel projectId="p" photoId="source" disabled={false} onSync={onSync} />);
  fireEvent.click(screen.getByText('Synchronize settings'));
  fireEvent.change(screen.getByLabelText('Copy to'), { target: { value: 'selected' } });
  for (const [id, label] of SYNC_GROUPS) if (id !== 'tone' && id !== 'geometry') fireEvent.click(screen.getByLabelText(label));
  fireEvent.click(screen.getByText('Apply selected settings'));
  expect(onSync).toHaveBeenCalledWith(['target'], ['tone']);
});
