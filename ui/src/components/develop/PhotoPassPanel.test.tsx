import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { PhotoPassPanel } from './PhotoPassPanel';
import { readPassReport, type PhotoPassReport } from '../../ipc/photoPass';
import type { RecipeDto } from '../../ipc/types';

const report: PhotoPassReport = {
  version: 'photo-pass-v1-batch1', planned: 100, applied: 1, historySteps: 1,
  summary: '1 of 3 steps changed the photograph; 1 found nothing to fix; 1 did not apply; 0 kept your own settings.',
  steps: [
    { number: 1, id: 'sensor_dust', title: 'Sensor dust', category: 'capture', outcome: 'applied', checks: ['2 specks of dust.'], changes: ['Healed 2 specks.'], operations: 2, saved: true },
    { number: 6, id: 'jpeg_compression', title: 'JPEG compression', category: 'capture', outcome: 'not_applicable', checks: ['Not a JPEG file.'], changes: [], operations: 0, saved: false },
    { number: 11, id: 'shadow_tint', title: 'Shadow tint', category: 'capture', outcome: 'protected', checks: ['Greener shadows.'], changes: ['You set this by hand.'], operations: 0, saved: false },
  ],
};
const recipe = (body: unknown) => ({ photoId: 'a', body: JSON.stringify(body) }) as RecipeDto;

it('reads the report saved in the recipe and ignores anything malformed', () => {
  expect(readPassReport(recipe({ photo_pass_v1: report }))).toEqual(report);
  expect(readPassReport(recipe({ global: {} }))).toBeNull();
  const odd = readPassReport(recipe({ photo_pass_v1: { ...report, steps: [{ number: 1, outcome: 'exploded' }, report.steps[0]] } }));
  expect(odd?.steps).toHaveLength(1);
  expect(readPassReport({ photoId: 'a', body: '{' } as RecipeDto)).toBeNull();
});

it('lists every step with what it found and runs again on request', () => {
  const run = vi.fn();
  render(<PhotoPassPanel recipe={recipe({ photo_pass_v1: report })} disabled={false} onRun={run} />);
  expect(screen.getByRole('status').textContent).toContain('3 of 100 steps built');
  const steps = screen.getByRole('list', { name: 'Photo pass steps' }).querySelectorAll(':scope > li');
  expect(steps).toHaveLength(3);
  expect(steps[0]?.textContent).toContain('1. Sensor dust · changed · 2 editable operations');
  expect(steps[1]?.textContent).toContain('not applicable');
  expect(steps[2]?.textContent).toContain('kept your setting');
  fireEvent.click(screen.getByRole('button', { name: 'Run photo pass again' }));
  expect(run).toHaveBeenCalledTimes(1);
});

it('offers the pass on a photo it has not run on', () => {
  render(<PhotoPassPanel recipe={recipe({})} disabled onRun={vi.fn()} />);
  expect(screen.getByRole('button', { name: 'Run photo pass' })).toHaveProperty('disabled', true);
  expect(screen.queryByRole('list', { name: 'Photo pass steps' })).toBeNull();
});
