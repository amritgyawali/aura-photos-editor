import { beforeEach, describe, expect, it, vi } from 'vitest';
import { renderSelectedLook } from './applyLook';
import { colour, tone } from '../../ipc/client';

vi.mock('../../ipc/client', () => ({
  tone: { estimateTone: vi.fn() }, colour: { estimateColour: vi.fn() },
}));
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(tone.estimateTone).mockResolvedValue({ recipesWritten: 3, failed: 0, cancelled: false } as never);
  vi.mocked(colour.estimateColour).mockResolvedValue({ recipesWritten: 3, failed: 0, cancelled: false } as never);
});
describe('applying a selected look', () => {
  it('runs tone before grading and reports saved recipes', async () => {
    const answer = await renderSelectedLook('project', 'job', () => false);
    expect(tone.estimateTone).toHaveBeenCalledWith({ projectId: 'project', cancelId: 'job' });
    expect(colour.estimateColour).toHaveBeenCalledWith({ projectId: 'project', cancelId: 'job' });
    expect(vi.mocked(tone.estimateTone).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(colour.estimateColour).mock.invocationCallOrder[0]!);
    expect(answer).toContain('3 tone edits and 3 color edits');
  });
  it('does not start grading after cancellation', async () => {
    vi.mocked(tone.estimateTone).mockResolvedValue({ recipesWritten: 1, failed: 0, cancelled: true } as never);
    expect(await renderSelectedLook('p', 'j', () => false)).toContain('Stopped');
    expect(colour.estimateColour).not.toHaveBeenCalled();
  });
  it('surfaces partial failures without claiming complete success', async () => {
    vi.mocked(colour.estimateColour).mockResolvedValue({ recipesWritten: 2, failed: 1, cancelled: false } as never);
    expect(await renderSelectedLook('p', 'j', () => false)).toContain('1 operations failed');
  });
  it('does no work if stop arrived between selection and the first pass', async () => {
    await renderSelectedLook('p', 'j', () => true);
    expect(tone.estimateTone).not.toHaveBeenCalled();
  });
  it('propagates write errors for the caller to display', async () => {
    vi.mocked(tone.estimateTone).mockRejectedValue(new Error('Original unavailable'));
    await expect(renderSelectedLook('p', 'j', () => false)).rejects.toThrow('Original unavailable');
    expect(colour.estimateColour).not.toHaveBeenCalled();
  });
});
