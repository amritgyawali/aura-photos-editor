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
  await waitFor(() => expect(advancedRetouch.run).toHaveBeenCalledWith('p', 'a', undefined, 'professional'));
  act(() => {
    push?.({ photoId: 'a', number: 1, total: 18, title: 'RAW foundation', state: 'done', outcome: 'applied' });
    push?.({ photoId: 'a', number: 2, total: 18, title: 'Lens & perspective', state: 'running', outcome: null });
    push?.({ photoId: 'other', number: 9, total: 18, title: 'x', state: 'running', outcome: null });
  });
  expect(screen.getByRole('button', { name: /Step 2 of 18 · Lens & perspective/ })).toBeTruthy();
  expect(document.querySelector('[aria-current="step"]')?.textContent).toContain('2. Lens & perspective');
  expect(screen.getAllByText(/Working…/)).toHaveLength(1);
  await act(async () => { finish?.({ recipe: { photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: report }) } as RecipeDto, report }); });
  await screen.findByText('All 18 stages ran in order.');
  expect(screen.getByText(/Skin texture kept: 83%/)).toBeTruthy();
  expect(screen.getAllByText(/Done · saved as its own step/).length).toBeGreaterThan(0);
  expect(screen.getByText(/Not applicable to this photo/)).toBeTruthy();
  expect(screen.getByText('Acne clear on 1 face', { selector: 'summary' })).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Run Auto advanced retouch again' })).toBeTruthy();
});

it('runs the Evoto-style Beauty & Fashion pass when that style is chosen', async () => {
  vi.mocked(advancedRetouch.onProgress).mockResolvedValue(() => {});
  vi.mocked(advancedRetouch.run).mockResolvedValue({ recipe: { photoId: 'a' } as RecipeDto, report });
  render(<AdvancedRetouch projectId="p" photoId="b" recipe={null} disabled={false} onRun={task => void task()} />);
  fireEvent.click(screen.getByRole('radio', { name: /Beauty & Fashion/ }));
  fireEvent.click(screen.getAllByRole('button', { name: 'Auto advanced retouch' })[0]!);
  await waitFor(() => expect(advancedRetouch.run).toHaveBeenCalledWith('p', 'b', undefined, 'beauty_fashion'));
});

it('reads the last run back from the recipe and ignores what it does not understand', () => {
  const recipe = { photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: {
    ...report, stages: [...report.stages, { number: 19, stage: 'liquify', outcome: 'applied', checks: [], changes: [] }, { number: 3, stage: 'background', outcome: 'magic' }],
  } }) } as RecipeDto;
  const parsed = readAdvancedReport(recipe);
  expect(parsed?.stages).toHaveLength(18);
  expect(parsed?.quality.textureRetention).toBeCloseTo(.83);
  expect(parsed?.quality.passed).toBe(false);
  expect(parsed?.quality.measurementStatus).toBe('not_measured');
  expect(readAdvancedReport({ photoId: 'a', body: '{}' } as RecipeDto)).toBeNull();
  expect(readAdvancedReport({ photoId: 'a', body: 'not json' } as RecipeDto)).toBeNull();
  render(<AdvancedRetouch projectId="p" photoId="a" recipe={recipe} disabled={false} onRun={vi.fn()} />);
  expect(screen.getByRole('button', { name: 'Run Auto advanced retouch again' })).toBeTruthy();
  expect(screen.getByText('All 18 stages ran in order.')).toBeTruthy();
});

it('does not accept a claimed pass with incomplete contour coverage or missing texture', () => {
  const quality = { ...report.quality, measurementStatus: 'measured', analysisWidth: 1200, analysisHeight: 800,
    contours: [{ face: 0, sampledPairs: 1200, newSeamPairs: 0, sourceEdgePairs: 100, weakenedEdgePairs: 0, review: false,
      regions: ['eye_image_left', 'eye_image_right', 'nose', 'mouth', 'chin_neck'].map(region => ({
        region, sampledPairs: 100, newSeamPairs: 0, sourceEdgePairs: 20, weakenedEdgePairs: 0, review: false, measured: true,
      })) }] };
  const read = (q: unknown, faces = 1) => readAdvancedReport({ photoId: 'a', body: JSON.stringify({
    studio_advanced_retouch_v1: { ...report, faces, quality: q, checkedRecipe: '{}', checkedPhotoId: 'a',
      summary: 'The measured checks passed at QC resolution; inspect full-resolution output before delivery.' },
  }) } as RecipeDto);
  expect(read(quality)?.quality.passed).toBe(true);
  expect(read({ ...quality, textureRetention: null })?.quality.passed).toBe(false);
  for (const invalid of [{ textureRetention: 0.2 }, { skinShift: 0.05 }, { skinShift: -0.1 }, { clippedFinal: 0.1 }]) {
    const parsed = read({ ...quality, ...invalid });
    expect(parsed?.quality.passed).toBe(false);
    expect(parsed?.summary).not.toContain('checks passed');
  }
  expect(read(quality, 2)?.quality.passed).toBe(false);
  expect(read({ ...quality, contours: [{ ...quality.contours[0], review: true }] })?.quality.passed).toBe(false);
  const local = (regions: unknown[]) => read({ ...quality, contours: [{ ...quality.contours[0], regions }] });
  const regions = quality.contours[0]!.regions;
  expect(local([])?.quality.passed).toBe(false);
  expect(local(regions.map((r, i) => i === 0 ? { ...r, review: true } : r))?.quality.passed).toBe(false);
  expect(local(regions.map((r, i) => i === 0 ? { ...r, review: true } : r))?.summary).toContain('requires review');
  expect(local([...regions.slice(0, 4), regions[0]])?.quality.passed).toBe(false);
  expect(local(regions.map((r, i) => i === 0 ? { ...r, measured: false } : r))?.quality.passed).toBe(false);
  expect(local(regions.map((r, i) => i === 0 ? { ...r, review: undefined } : r))?.quality.passed).toBe(false);
  const reviewRecipe = { photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: {
    ...report, faces: 1, quality: { ...quality, contours: [{ ...quality.contours[0],
      regions: regions.map((r, i) => i === 0 ? { ...r, review: true } : r) }] },
  } }) } as RecipeDto;
  render(<AdvancedRetouch projectId="p" photoId="a" recipe={reviewRecipe} disabled={false} onRun={vi.fn()} />);
  expect(screen.getByRole('table', { name: 'Facial region contour checks' })).toBeTruthy();
  expect(screen.getByText('Image-left eye / under-eye')).toBeTruthy();
  expect(screen.getByText('Review required')).toBeTruthy();
  const legacy = readAdvancedReport({ photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: {
    ...report, summary: 'All stages ran. Quality control passed.',
  } }) } as RecipeDto);
  expect(legacy?.summary).toContain('lacks complete QC measurements');
});

it('invalidates a saved check after colour, crop, retouch or photo identity changes', () => {
  const settings = { image: { content_hash: 'original' }, global: { exposure: 0.123457 },
    geometry: { rotate: 0 }, studio_retouch_tools_v1: { edits: [{ amount: 0.3 }] } };
  const saved = { ...report, checkedRecipe: JSON.stringify(settings), checkedPhotoId: 'a',
    summary: 'The measured checks passed at QC resolution; inspect full-resolution output before delivery.' };
  const read = (body: unknown, photoId = 'a') => readAdvancedReport({ photoId,
    body: JSON.stringify({ ...(body as object), studio_advanced_retouch_v1: saved }) } as RecipeDto);
  expect(read(settings)?.revisionStatus).toBe('current');
  expect(read({ ...settings, provenance: { source: 'user' } })?.revisionStatus).toBe('current');
  expect(read({ ...settings, global: { exposure: 0.12345699965953827 } })?.revisionStatus).toBe('current');
  for (const changed of [
    { ...settings, global: { exposure: 0.5 } },
    { ...settings, geometry: { rotate: 2 } },
    { ...settings, studio_retouch_tools_v1: { edits: [{ amount: 0.6 }] } },
    { ...settings, image: { content_hash: 'different-source' } },
  ]) {
    const result = read(changed);
    expect(result?.revisionStatus).toBe('changed');
    expect(result?.quality.passed).toBe(false);
    expect(result?.summary).toContain('settings changed');
    expect(result?.summary).not.toContain('checks passed');
  }
  expect(read(settings, 'other-photo')?.revisionStatus).toBe('changed');
});

it('ignores a previous photo run that finishes after navigating to another photo', async () => {
  let finish: ((value: { recipe: RecipeDto; report: AdvancedRetouchReport }) => void) | undefined;
  vi.mocked(advancedRetouch.onProgress).mockResolvedValue(() => {});
  vi.mocked(advancedRetouch.run).mockImplementation(() => new Promise(resolve => { finish = resolve; }));
  const props = { projectId: 'p', disabled: false, onRun: (task: () => Promise<void>) => void task() };
  const view = render(<AdvancedRetouch {...props} photoId="a" recipe={null} />);
  fireEvent.click(screen.getByRole('button', { name: 'Auto advanced retouch' }));
  await waitFor(() => expect(advancedRetouch.run).toHaveBeenCalled());
  view.rerender(<AdvancedRetouch {...props} photoId="b" recipe={null} />);
  await act(async () => { finish?.({ recipe: { photoId: 'a', body: JSON.stringify({ studio_advanced_retouch_v1: report }) } as RecipeDto, report }); });
  expect(screen.queryByText('All 18 stages ran in order.')).toBeNull();
  expect(screen.getByRole('button', { name: 'Auto advanced retouch' })).toBeTruthy();
});

it('shows the saved report becoming stale immediately when settings are edited', () => {
  const settings = { global: { exposure: 0 } };
  const saved = { ...report, checkedPhotoId: 'a', checkedRecipe: JSON.stringify(settings) };
  const recipe = (exposure: number) => ({ photoId: 'a', body: JSON.stringify({
    global: { exposure }, studio_advanced_retouch_v1: saved,
  }) } as RecipeDto);
  const props = { projectId: 'p', photoId: 'a', disabled: false, onRun: vi.fn() };
  const view = render(<AdvancedRetouch {...props} recipe={recipe(0)} />);
  expect(screen.queryByText(/These measurements belong to earlier settings/)).toBeNull();
  view.rerender(<AdvancedRetouch {...props} recipe={recipe(0.5)} />);
  expect(screen.getByText(/These measurements belong to earlier settings/)).toBeTruthy();
  expect(screen.queryByText('All 18 stages ran in order.')).toBeNull();
});
