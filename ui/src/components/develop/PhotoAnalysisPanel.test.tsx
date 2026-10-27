import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { PhotoAnalysisPanel } from './PhotoAnalysisPanel';
import { photoAnalysis, type PhotoAnalysisReport } from '../../ipc/photoAnalysis';
import type { RecipeDto } from '../../ipc/types';
import { QUALITY_GROUPS } from '../../ipc/qualityTools';

vi.mock('../../ipc/photoAnalysis', () => ({ photoAnalysis: { inspect: vi.fn() } }));
const recipe = { photoId: 'photo', recipeHash: 'revision' } as RecipeDto;
function report(recipeHash = 'revision'): PhotoAnalysisReport {
  return { recipeHash, width: 96, height: 64, notes: [], diagnostics: [
    { id: 'rgb-parade', title: 'RGB parade', description: 'Three channels.', kind: 'density', unit: 'samples', width: 2, height: 1, values: [2, 5], labels: [], low: 2, high: 5, samples: 7 },
    { id: 'neutral-cast', title: 'Near-neutral colour balance', description: 'Gain.', kind: 'bars', unit: 'ratio', width: 3, height: 1, values: [1.1, 1, 0.9], labels: ['Red', 'Green', 'Blue'], low: 0.9, high: 1.1, samples: 7 },
    { id: 'edge-registration', title: 'Neutral-edge channel alignment', description: 'Offset.', kind: 'bars', unit: 'px', width: 2, height: 1, values: [0, 0], labels: ['Red–green', 'Blue–green'], low: 0, high: 0, samples: 0 },
  ] };
}
beforeEach(() => { vi.mocked(photoAnalysis.inspect).mockReset(); });

it('runs explicitly with the saved revision and exposes scopes and colour measurements', async () => {
  vi.mocked(photoAnalysis.inspect).mockResolvedValue(report());
  render(<PhotoAnalysisPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} />);
  expect(photoAnalysis.inspect).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Run photo inspection'));
  await screen.findByRole('img', { name: 'RGB parade chart' });
  expect(photoAnalysis.inspect).toHaveBeenCalledWith('project', 'photo', 'revision');
  fireEvent.change(screen.getByLabelText('Inspection group'), { target: { value: 'Exposure & colour' } });
  expect(screen.getByRole('img', { name: 'Near-neutral colour balance chart' })).toBeTruthy();
  expect(screen.getByText('1.1 ratio')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Inspection group'), { target: { value: 'Detail & artifacts' } });
  expect(screen.getByText('No usable samples for this measurement.')).toBeTruthy();
});
it('discards a result from a previous photo and clears measurements after saved edits change', async () => {
  let resolve!: (value: PhotoAnalysisReport) => void;
  vi.mocked(photoAnalysis.inspect).mockReturnValue(new Promise(done => { resolve = done; }));
  const view = render(<PhotoAnalysisPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} />);
  fireEvent.click(screen.getByText('Run photo inspection'));
  view.rerender(<PhotoAnalysisPanel projectId="project" photoId="other" recipe={{ ...recipe, recipeHash: 'next' }} disabled={false} />);
  resolve(report());
  await waitFor(() => expect(screen.queryByRole('img')).toBeNull());
  expect(screen.queryByLabelText('Measurement')).toBeNull();
  vi.mocked(photoAnalysis.inspect).mockResolvedValue(report('next'));
  fireEvent.click(screen.getByText('Run photo inspection'));
  await screen.findByRole('img', { name: 'RGB parade chart' });
  view.rerender(<PhotoAnalysisPanel projectId="project" photoId="other" recipe={{ ...recipe, recipeHash: 'third' }} disabled={false} />);
  expect(screen.queryByRole('img')).toBeNull();
});
it('shows stale and native failures and prevents requests when unavailable', async () => {
  vi.mocked(photoAnalysis.inspect).mockResolvedValue(report('wrong-revision'));
  const view = render(<PhotoAnalysisPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} />);
  fireEvent.click(screen.getByText('Run photo inspection'));
  await screen.findByText('Photo changed. Run the inspection again.');
  expect(screen.queryByRole('img')).toBeNull();
  vi.mocked(photoAnalysis.inspect).mockRejectedValue({ message: 'Source unavailable', code: 'AURA-RENDER-8002', runbookUrl: 'https://aura.app/e/AURA-RENDER-8002', retryable: false });
  fireEvent.click(screen.getByText('Run photo inspection'));
  await screen.findByText('Source unavailable');
  view.rerender(<PhotoAnalysisPanel projectId="project" photoId="photo" recipe={null} disabled={false} />);
  expect((screen.getByText('Run photo inspection') as HTMLButtonElement).disabled).toBe(true);
});

it('exposes all 75 advanced measurements and retains one revision-bound report across review screens', async () => {
  const data = report();
  data.diagnostics.push(...QUALITY_GROUPS.flatMap(([, ids]) => ids.map(id => ({
    id, title: id, description: 'Native measurement.', kind: 'bars' as const, unit: 'signal',
    width: 1, height: 1, values: [0.2], labels: [], low: 0.2, high: 0.2, samples: 12,
  }))));
  vi.mocked(photoAnalysis.inspect).mockResolvedValue(data);
  const props = { projectId: 'project', photoId: 'photo', recipe, disabled: false };
  const view = render(<PhotoAnalysisPanel {...props} focusGroup="Palette & tone review" />);
  fireEvent.click(screen.getByText('Run photo inspection'));
  await screen.findByRole('img', { name: 'channel-information chart' });
  for (const [group, ids] of QUALITY_GROUPS) {
    view.rerender(<PhotoAnalysisPanel {...props} focusGroup={group} />);
    expect((screen.getByLabelText('Inspection group') as HTMLSelectElement).value).toBe(group);
    const select = screen.getByLabelText('Measurement');
    expect(select.querySelectorAll('option')).toHaveLength(ids.length);
    for (const id of ids) {
      fireEvent.change(select, { target: { value: id } });
      expect(screen.getByRole('img', { name: `${id} chart` })).toBeTruthy();
    }
  }
  expect(photoAnalysis.inspect).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('status').textContent).toContain('78 measurements');
});
