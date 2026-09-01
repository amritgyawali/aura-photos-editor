import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { AutopilotEventDto, AutopilotSummaryDto } from '../../ipc/types';
import { RunSummary } from './RunSummary';

/**
 * PHASE-28. Every test here is about **what did not happen being the first thing on the screen**.
 *
 * A photographer opening this at one in the morning is not looking for how many photographs were
 * selected. They are looking for what did not run, and the honest version of that is a list with a
 * sentence per row rather than a number - which is why `degradedStages` is a list on the wire.
 */

function summary(overrides: Partial<AutopilotSummaryDto> = {}): AutopilotSummaryDto {
  return {
    runId: 'run_1',
    status: 'completed',
    statusTitle: 'Finished',
    selected: 420,
    exported: 0,
    needsReview: 12,
    totalMs: 7_200_000,
    spendUsd: 0,
    outputPath: '',
    stageTimings: [],
    degradedStages: [],
    ...overrides,
  };
}

function event(overrides: Partial<AutopilotEventDto> = {}): AutopilotEventDto {
  return {
    kind: 'thermal',
    action: 'reduce',
    actionText: 'Reducing speed to protect your machine',
    reading: 88,
    threshold: 85,
    stage: 'previews',
    ...overrides,
  };
}

describe('RunSummary', () => {
  it('says so when the wedding has not been run', () => {
    render(<RunSummary summary={null} events={[]} />);
    expect(screen.getByText('This wedding has not been run yet.')).toBeTruthy();
  });

  it('names every step that did not do what it was meant to, with its reason', () => {
    render(
      <RunSummary
        summary={summary({
          status: 'completed_degraded',
          statusTitle: 'Finished, with some steps skipped',
          degradedStages: [
            ['curation', 'This release does not include this step yet'],
            ['export', 'This release does not include this step yet'],
          ],
        })}
        events={[]}
      />,
    );
    expect(screen.getByText('What did not happen')).toBeTruthy();
    expect(screen.getByText('curation')).toBeTruthy();
    expect(screen.getByText('export')).toBeTruthy();
    expect(screen.getAllByText('This release does not include this step yet')).toHaveLength(2);
  });

  it('never renders a degraded run as simply finished', () => {
    // `CompletedDegraded` is a real outcome with a real meaning, and a green tick would be a panel
    // lying by omission.
    const { container } = render(
      <RunSummary summary={summary({ status: 'completed_degraded' })} events={[]} />,
    );
    expect(container.querySelector('.autopilot-summary')?.className).toContain('is-degraded');
  });

  it('shows no skipped section when nothing was skipped', () => {
    render(<RunSummary summary={summary()} events={[]} />);
    expect(screen.queryByText('What did not happen')).toBeNull();
  });

  it('reports what the machine asked for, so a slow run has an explanation', () => {
    render(<RunSummary summary={summary()} events={[event(), event({ kind: 'quiet' })]} />);
    expect(screen.getByText('What your machine asked for')).toBeTruthy();
    expect(screen.getAllByText('Reducing speed to protect your machine')).toHaveLength(2);
  });

  it('hides the machine section when the governor did nothing', () => {
    render(<RunSummary summary={summary()} events={[]} />);
    expect(screen.queryByText('What your machine asked for')).toBeNull();
  });

  it('shows the five slowest steps rather than all twenty-five', () => {
    const timings: [string, number][] = [
      ['ingest', 1000],
      ['previews', 9000],
      ['embed', 8000],
      ['faces', 7000],
      ['story', 6000],
      ['moments', 5000],
    ];
    render(<RunSummary summary={summary({ stageTimings: timings })} events={[]} />);
    expect(screen.getByText('Where the time went')).toBeTruthy();
    expect(screen.queryByText('ingest')).toBeNull();
    expect(screen.getByText('previews')).toBeTruthy();
  });

  it('does not claim an output path when nothing was written', () => {
    // This build has no exporter, so `outputPath` is empty and `exported` is zero. A panel that
    // showed a folder would send a photographer looking for files that are not there.
    render(<RunSummary summary={summary()} events={[]} />);
    expect(screen.queryByText(/Delivered files/)).toBeNull();
  });
});
