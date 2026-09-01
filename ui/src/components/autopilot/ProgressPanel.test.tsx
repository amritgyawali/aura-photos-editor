import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { AutopilotProgressDto } from '../../ipc/types';
import { humanDuration, ProgressPanel } from './ProgressPanel';

/**
 * PHASE-28. Every test here is about **the ETA saying whether it was measured**.
 *
 * Before this machine has measured its own throughput, the number comes from an estimate the phase
 * document wrote for a reference laptop. Those are two different claims, and a panel that showed
 * them identically would promise two hours on a machine doing four - which a photographer would
 * find out about at hour three.
 */

function progress(overrides: Partial<AutopilotProgressDto> = {}): AutopilotProgressDto {
  return {
    runId: 'run_1',
    status: 'running',
    stage: 'previews',
    stageTitle: 'Building previews',
    stageIndex: 1,
    stageTotal: 25,
    itemsDone: 200,
    itemsTotal: 1000,
    etaS: 400,
    throughputPerS: 2,
    spendUsd: 0,
    warnings: [],
    currentImage: null,
    cancelled: false,
    ...overrides,
  };
}

describe('ProgressPanel', () => {
  it('says nothing is running when nothing is', () => {
    render(<ProgressPanel progress={null} />);
    expect(screen.getByText('Nothing is running.')).toBeTruthy();
  });

  it('shows a time only once this machine has measured its own speed', () => {
    render(<ProgressPanel progress={progress()} />);
    expect(screen.getByText('about 7 minutes')).toBeTruthy();
  });

  it('refuses to show a time before the first measurement', () => {
    // The failure this component exists to avoid. A throughput of zero is the honest signal, and
    // an ETA of zero is not - that is also a run about to finish.
    render(<ProgressPanel progress={progress({ throughputPerS: 0, etaS: 8400 })} />);
    expect(screen.getByText('working out how long this will take')).toBeTruthy();
    expect(screen.queryByText(/about 2 hours/)).toBeNull();
  });

  it('reports overall progress across the whole plan rather than one stage', () => {
    // Stage 1 of 25, one fifth through it: (1 + 0.2) / 25 = 4.8 %, which rounds to 5.
    render(<ProgressPanel progress={progress()} />);
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('5');
  });

  it('says what stopping means rather than only that it is stopping', () => {
    render(<ProgressPanel progress={progress({ cancelled: true })} />);
    expect(screen.getByText(/Everything done so far is saved/)).toBeTruthy();
  });

  it('shows the spend meter even though this phase makes no cloud call of its own', () => {
    // The stages can - phase 24's judgement and phase 27's planner both reach a provider - and a
    // run that spent a photographer's money without a meter would be one they found out about on a
    // bill.
    render(<ProgressPanel progress={progress({ spendUsd: 1.5 })} />);
    expect(screen.getByText('$1.50')).toBeTruthy();
  });

  it('does not divide by zero on a stage with no units', () => {
    render(<ProgressPanel progress={progress({ itemsDone: 0, itemsTotal: 0, stageIndex: 0 })} />);
    expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('0');
  });
});

describe('humanDuration', () => {
  it('never says zero', () => {
    expect(humanDuration(0)).toBe('less than a minute');
    expect(humanDuration(-5)).toBe('less than a minute');
  });

  it('speaks in the units a person would', () => {
    expect(humanDuration(60)).toBe('about a minute');
    expect(humanDuration(600)).toBe('about 10 minutes');
    expect(humanDuration(3600)).toBe('about an hour');
    expect(humanDuration(9000)).toBe('about 2h 30m');
  });
});
