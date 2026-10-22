import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { WorkflowLabPanel } from './WorkflowLabPanel';
import { workflowLab, type WorkflowSnapshot } from '../../ipc/workflowLab';
import { develop } from '../../ipc/client';
import type { RecipeDto } from '../../ipc/types';

vi.mock('../../ipc/workflowLab', () => ({ workflowLab: { run: vi.fn() } }));
vi.spyOn(develop, 'renderImage');
const recipe = { photoId: 'first', recipeHash: 'first-hash' } as RecipeDto;
function data(): WorkflowSnapshot {
  return { revision: 'workspace-revision', queue: ['second', 'first'], criteria: ['Colour', 'Detail'], reviews: {},
    issues: [{ id: 'issue', photoId: 'first', recipeHash: 'first-hash', text: 'Dust', point: [0.2, 0.3], resolved: false, dependsOn: [] }], comparisons: [], filters: [],
    checkpoints: [{ id: 'checkpoint', name: 'Baseline', at: '2026-10-20', count: 2 }], plan: null, audit: [],
    photos: ['first', 'second'].map(id => ({ photoId: id, recipeHash: `${id}-hash`, status: 'unreviewed', openIssues: 0, overdue: false, fingerprint: 'same-settings', sections: { tone: 'tone', colour: 'colour' } })),
    rows: [], selected: [], document: null, message: '' };
}
beforeEach(() => { vi.mocked(workflowLab.run).mockReset(); vi.mocked(develop.renderImage).mockReset(); });
async function mounted(onEdited = vi.fn()) {
  vi.mocked(workflowLab.run).mockResolvedValue(data());
  const view = render(<WorkflowLabPanel projectId="project" photoId="first" photoIds={['first', 'second']} recipe={recipe} disabled={false} onEdited={onEdited} />);
  expect(workflowLab.run).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Load review workspace'));
  await screen.findByText('Review queue & checklist'); return { view, onEdited };
}
it('loads explicitly and sends native review actions with all selected revision hashes', async () => {
  await mounted();
  fireEvent.click(screen.getByText('Pass criterion for cohort'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ action: 'signoff', projectId: 'project', photoIds: ['first', 'second'], expectedRevision: 'workspace-revision', recipeHashes: { first: 'first-hash', second: 'second-hash' }, criterion: 'Colour', passed: true })));
  fireEvent.change(screen.getByLabelText('Review owner'), { target: { value: 'Alex' } });
  fireEvent.click(screen.getByText('Assign selected review workload'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ action: 'assign', reviewer: 'Alex' })));
  fireEvent.change(screen.getByLabelText('Workflow photograph IDs'), { target: { value: 'first' } });
  expect((screen.getByText('Pass criterion for cohort') as HTMLButtonElement).disabled).toBe(true);
});
it('exposes checkpoint planning separately from apply and blocks conflicted plans', async () => {
  const { onEdited } = await mounted(); fireEvent.click(screen.getByRole('button', { name: 'Checkpoints' }));
  fireEvent.click(screen.getByText('Plan section restore from checkpoint'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ action: 'restore_plan', checkpointId: 'checkpoint', sections: ['tone'] })));
  const planned = data(); planned.plan = { kind: 'restore_plan', entries: [{ photoId: 'first', changed: ['global.exposure'], conflicts: [] }] };
  vi.mocked(workflowLab.run).mockResolvedValue(planned);
  fireEvent.click(screen.getByText('Plan section restore from checkpoint')); await screen.findByText('Apply reviewed batch plan');
  expect(onEdited).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Apply reviewed batch plan')); await waitFor(() => expect(onEdited).toHaveBeenCalledOnce());
  const entry = planned.plan.entries[0]; if (!entry) throw new Error('Missing planned entry'); entry.conflicts = ['tone'];
  fireEvent.click(screen.getByText('Plan baseline-aware three-way transfer'));
  await waitFor(() => expect((screen.getByText('Apply reviewed batch plan') as HTMLButtonElement).disabled).toBe(true));
});
it('builds portable metadata and handoff without sending or applying edits', async () => {
  await mounted(); fireEvent.click(screen.getByRole('button', { name: 'Handoff' }));
  const result = data(); result.document = '{"format":"aura-review-v1"}';
  vi.mocked(workflowLab.run).mockResolvedValue(result);
  fireEvent.click(screen.getByText('Export review metadata'));
  await waitFor(() => expect((screen.getByLabelText('Review interchange document') as HTMLTextAreaElement).value).toContain('aura-review-v1'));
  fireEvent.click(screen.getByText('Import review metadata and reopen approvals'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ action: 'import', document: result.document })));
});
it('rejects late results after navigation and shows native failures', async () => {
  let resolve!: (value: WorkflowSnapshot) => void;
  vi.mocked(workflowLab.run).mockReturnValue(new Promise(done => { resolve = done; }));
  const view = render(<WorkflowLabPanel projectId="project" photoId="first" recipe={recipe} disabled={false} />);
  fireEvent.click(screen.getByText('Load review workspace'));
  view.rerender(<WorkflowLabPanel projectId="project" photoId="next" recipe={{ ...recipe, recipeHash: 'next-hash' }} disabled={false} />);
  resolve(data()); await waitFor(() => expect(screen.queryByText('Review queue & checklist')).toBeNull());
  vi.mocked(workflowLab.run).mockRejectedValue({ code: 'AURA-RENDER-8002', message: 'Refresh the workspace.', runbookUrl: 'https://aura.app/e/AURA-RENDER-8002', retryable: false });
  fireEvent.click(screen.getByText('Load review workspace')); await screen.findByText('Refresh the workspace.');
});
it('renders a saved edit pair only after verifying revisions before and after rendering', async () => {
  await mounted(); fireEvent.click(screen.getByRole('button', { name: 'Compare' }));
  vi.mocked(develop.renderImage).mockResolvedValue({ width: 1, height: 1, rgbBase64: 'AQID' } as Awaited<ReturnType<typeof develop.renderImage>>);
  fireEvent.click(screen.getByText('Preview saved edit pair'));
  await screen.findByAltText('Left saved edit'); expect(screen.getByAltText('Right saved edit')).toBeTruthy();
  expect(develop.renderImage).toHaveBeenCalledTimes(2);
  expect(vi.mocked(workflowLab.run).mock.calls.filter(([input]) => input.action === 'verify')).toHaveLength(2);
});

it('reloads current saved criteria and recovers after the workspace changes elsewhere', async () => {
  await mounted();
  expect((screen.getByLabelText('Review checklist criteria') as HTMLTextAreaElement).value).toBe('Colour\nDetail');
  fireEvent.change(screen.getByLabelText('Signoff criterion'), { target: { value: 'Detail' } });
  vi.mocked(workflowLab.run).mockRejectedValueOnce({ message: 'Reload the workspace.', code: 'AURA-RENDER-8002' });
  fireEvent.click(screen.getByText('Pass criterion for cohort'));
  await screen.findByRole('alert');
  const newer = data(); newer.revision = 'newer-revision'; newer.criteria = ['Export quality'];
  vi.mocked(workflowLab.run).mockResolvedValue(newer);
  fireEvent.click(screen.getByText('Load review workspace'));
  await waitFor(() => expect((screen.getByLabelText('Signoff criterion') as HTMLSelectElement).value).toBe('Export quality'));
  const request = vi.mocked(workflowLab.run).mock.calls.at(-1)![0];
  expect(request.action).toBe('read');
  expect(request.expectedRevision).toBeUndefined();
  expect(request.recipeHashes).toBeUndefined();
  fireEvent.click(screen.getByText('Pass criterion for cohort'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ criterion: 'Export quality', expectedRevision: 'newer-revision' })));
});

it('reconciles removed checkpoints and a transfer source removed from the cohort', async () => {
  await mounted(); fireEvent.click(screen.getByRole('button', { name: 'Checkpoints' }));
  fireEvent.change(screen.getByLabelText('Saved cohort checkpoint'), { target: { value: 'checkpoint' } });
  const newer = data(); newer.checkpoints = [{ id: 'remaining', name: 'Remaining baseline', at: '2026-10-21', count: 2 }];
  vi.mocked(workflowLab.run).mockResolvedValue(newer);
  fireEvent.click(screen.getByText('Remove selected checkpoint'));
  await waitFor(() => expect((screen.getByLabelText('Saved cohort checkpoint') as HTMLSelectElement).value).toBe('remaining'));
  fireEvent.change(screen.getByLabelText('Workflow photograph IDs'), { target: { value: 'second' } });
  newer.photos = newer.photos.filter(photo => photo.photoId === 'second');
  fireEvent.click(screen.getByText('Load review workspace'));
  await waitFor(() => expect((screen.getByText('Plan relative tone and colour transfer') as HTMLButtonElement).disabled).toBe(false));
  expect((screen.getByLabelText('Workflow transfer source') as HTMLSelectElement).value).toBe('second');
  fireEvent.click(screen.getByText('Plan relative tone and colour transfer'));
  await waitFor(() => expect(workflowLab.run).toHaveBeenLastCalledWith(expect.objectContaining({ checkpointId: 'remaining', sourcePhotoId: 'second' })));
});

it('clears a pair preference and interchange document when navigating to another photograph', async () => {
  const { view } = await mounted();
  fireEvent.click(screen.getByRole('button', { name: 'Compare' }));
  fireEvent.change(screen.getByLabelText('Pair preference'), { target: { value: 'first' } });
  fireEvent.change(screen.getByLabelText('Workflow photograph IDs'), { target: { value: 'second\nfirst' } });
  expect((screen.getByLabelText('Pair preference') as HTMLSelectElement).value).toBe('');
  fireEvent.click(screen.getByRole('button', { name: 'Handoff' }));
  fireEvent.change(screen.getByLabelText('Review interchange document'), { target: { value: 'prior document' } });
  view.rerender(<WorkflowLabPanel projectId="other-project" photoId="first" photoIds={['first', 'second']} recipe={recipe} disabled={false} />);
  fireEvent.click(screen.getByText('Load review workspace'));
  await screen.findByLabelText('Review interchange document');
  expect((screen.getByLabelText('Review interchange document') as HTMLTextAreaElement).value).toBe('');
});
