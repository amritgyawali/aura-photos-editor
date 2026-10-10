import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { AdvancedRetouch } from './AdvancedRetouch';
import { ADVANCED_STAGES, advancedRetouch, readAdvancedReport, type AdvancedRetouchProgress, type AdvancedRetouchReport } from '../../ipc/advancedRetouch';
import type { RecipeDto } from '../../ipc/types';

vi.mock('../../ipc/advancedRetouch', async () => ({
  ...await vi.importActual('../../ipc/advancedRetouch'),
  advancedRetouch: { run: vi.fn(), onProgress: vi.fn() },
}));

const report: AdvancedRetouchReport = {
  version: 'auto-advanced-v1', faces: 1, historySteps: 3, summary: 'All 18 stages ran in order.',
  quality: { textureRetention: .83, skinShift: .005, clippedOriginal: 0, clippedFinal: .001, mirrorBalance: 1.4, passed: true, corrected: false },
  stages: ADVANCED_STAGES.map(([stage, title], i) => ({
    number: i + 1, stage, title, operations: stage === 'skin_cleanup' ? 2 : 0, saved: stage === 'skin_cleanup',
    outcome: stage === 'skin_cleanup' ? 'applied' : stage === 'clothing' ? 'not_applicable' : 'unchanged',
    checks: [`checked ${stage}`], changes: stage === 'skin_cleanup' ? ['Acne clear on 1 face'] : [],
  })),
};

beforeEach(() => { vi.resetAllMocks(); });

it('lists all eighteen steps in the professional order before anything runs', () => {
  render(<AdvancedRetouch projectId="p" photoId="a" recipe={null} disabled={false} onRun={vi.fn()} />);
  const steps = screen.getByRole('list', { name: 'Advanced retouch steps' }).querySelectorAll(':scope > li');
  expect(steps).toHaveLength(18);
  expect(steps[0]?.textContent).toContain('1. RAW foundation');
  expect(steps[4]?.textContent).toContain('5. Skin cleanup');
  expect(steps[14]?.textContent).toContain('15. Colour grade');
  expect(steps[17]?.textContent).toContain('18. Quality control & export');
});

it('shows each step as it runs, then what every step checked and changed', async () => {
  let push: ((e: AdvancedRetouchProgress) => void) | null = null;
  vi.mocked(advancedRetouch.onProgress).mockImplementation(async handler => { push = handler; return () => {}; });
  let finish: ((value: { recipe: RecipeDto; report: AdvancedRetouchReport }) => void) | null = null;
  vi.mocked(advancedRetouch.run).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  render(<AdvancedRetouch projectId="p" photoId="a" recipe={null} disabled={false} onRun={task => void task()} />);
  fireEvent.click(screen.getByRole('button', { name: 'Auto advanced retouch' }));
  await waitFor(() => expect(advancedRetouch.run).toHaveBeenCalledWith('p', 'a'));
  act(() => {
    push?.({ photoId: 'a', number: 1, total: 18, title: 'RAW foundation', state: 'done', outcome: 'applied' });
    push?.({ photoId: 'a', number: 2, total: 18, title: 'Lens & perspective', state: 'running', outcome: null });
    push?.({ photoId: 'other', number: 9, total: 18, title: 'x', state: 'running', outcome: null });
  });
  expect(screen.getByRole('button', { name: /Step 2 of 18 · Lens & perspective/ })).toBeTruthy();
  expect(document.querySelector('[aria-current="step"]')?.textContent).toContain('2. Lens & perspective');
  expect(screen.getAllByText(/Working…/)).toHaveLength(1);
  await act(async () => { finish?.({ recipe: { photoId: 'a' } as RecipeDto, report }); });
  await screen.findByText('All 18 stages ran in order.');
  expect(screen.getByText(/Skin texture kept: 83%/)).toBeTruthy();
  expect(screen.getAllByText(/Done · saved as its own step/).length).toBeGreaterThan(0);
  expect(screen.getByText(/Not applicable to this photo/)).toBeTruthy();
  expect(screen.getByText('Acne clear on 1 face', { selector: 'summary' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Run Auto advanced retouch again' })).toBeTruthy();
});

it('reads the last run back from the recipe and ignores what it does not understand', () => {
  const recipe = { photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: {
    ...report, stages: [...report.stages, { number: 19, stage: 'liquify', outcome: 'applied', checks: [], changes: [] }, { number: 3, stage: 'background', outcome: 'magic' }],
  } }) } as RecipeDto;
  const parsed = readAdvancedReport(recipe);
  expect(parsed?.stages).toHaveLength(18);
  expect(parsed?.quality.textureRetention).toBeCloseTo(.83);
  expect(readAdvancedReport({ photoId: 'a', body: '{}' } as RecipeDto)).toBeNull();
  expect(readAdvancedReport({ photoId: 'a', body: 'not json' } as RecipeDto)).toBeNull();
  render(<AdvancedRetouch projectId="p" photoId="a" recipe={recipe} disabled={false} onRun={vi.fn()} />);
  expect(screen.getByRole('button', { name: 'Run Auto advanced retouch again' })).toBeTruthy();
  expect(screen.getByText('All 18 stages ran in order.')).toBeTruthy();
});
