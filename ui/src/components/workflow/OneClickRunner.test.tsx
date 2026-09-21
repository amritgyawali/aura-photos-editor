import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { oneClickFinish, oneClickStatus } from '../../ipc/client';
import type { OneClickStatusDto } from '../../ipc/types';
import { useStore } from '../../state/store';
import { useAutomatic } from '../../state/automaticStore';
import { OneClickRunner } from './OneClickRunner';

vi.mock('../../ipc/client', async (importOriginal) => {
  const original = await importOriginal<typeof import('../../ipc/client')>();
  return {
    ...original,
    inTauri: () => true,
    oneClickFinish: vi.fn(),
    oneClickStatus: vi.fn(),
    oneClickCancel: vi.fn(),
  };
});

function row(status: string): OneClickStatusDto {
  return {
    jobId: 'j-1',
    status,
    phase: status === 'running' ? 'edit' : 'done',
    phaseLabel: 'AI-editing the gallery.',
    itemsDone: 3,
    itemsTotal: 10,
    frames: 10,
    aiEdited: 3,
    localEdited: 0,
    selected: 10,
    written: 0,
    verified: 0,
    destination: 'D:/out',
    model: 'qwen3.8-flash',
    notes: ['provider compat at https://api.b.ai - cloud on for this run'],
  };
}

describe('OneClickRunner', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useAutomatic.setState({ starting: false, jobId: null, status: null });
    useStore.getState().setActiveProject('p-1');
    vi.mocked(oneClickStatus).mockResolvedValue(row('running'));
  });

  it('needs a destination before it will press the button', () => {
    render(<OneClickRunner onFinished={() => undefined} />);
    expect(screen.getByRole('button', { name: 'Finish everything' }).hasAttribute('disabled')).toBe(
      true,
    );
  });

  it('starts the pipeline at the chosen destination', async () => {
    vi.mocked(oneClickFinish).mockResolvedValue({ jobId: 'j-1' });
    render(<OneClickRunner onFinished={() => undefined} />);
    fireEvent.change(screen.getByLabelText('Destination folder'), { target: { value: 'D:/out' } });
    fireEvent.click(screen.getByRole('button', { name: 'Finish everything' }));
    expect(oneClickFinish).toHaveBeenCalledWith({
      projectId: 'p-1',
      destination: 'D:/out',
      ingestJobId: null,
    });
    await waitFor(() => expect(oneClickStatus).toHaveBeenCalledWith('j-1'));
    expect(useAutomatic.getState().jobId).toBe('j-1');
  });

  it('renders the phase, the counts and the notes as the row moves', async () => {
    vi.mocked(oneClickFinish).mockResolvedValue({ jobId: 'j-1' });
    vi.mocked(oneClickStatus).mockResolvedValue(row('completed'));
    render(<OneClickRunner onFinished={() => undefined} />);
    fireEvent.change(screen.getByLabelText('Destination folder'), { target: { value: 'D:/out' } });
    fireEvent.click(screen.getByRole('button', { name: 'Finish everything' }));
    expect(await screen.findByText(/Delivered to D:\/out/)).toBeDefined();
    expect(screen.getByText(/3 AI-edited/)).toBeDefined();
    expect(screen.getByText(/What the pipeline said/)).toBeDefined();
  });

  it('keeps the run busy while cancellation is still finishing', async () => {
    vi.mocked(oneClickFinish).mockResolvedValue({ jobId: 'j-1' });
    vi.mocked(oneClickStatus).mockResolvedValue(row('cancelling'));
    const onFinished = vi.fn();
    render(<OneClickRunner onFinished={onFinished} />);
    fireEvent.change(screen.getByLabelText('Destination folder'), { target: { value: 'D:/out' } });
    fireEvent.click(screen.getByRole('button', { name: 'Finish everything' }));
    await screen.findByText(/3 AI-edited/);
    expect(screen.getByLabelText('Destination folder').hasAttribute('disabled')).toBe(true);
    expect(onFinished).not.toHaveBeenCalled();
  });
});
