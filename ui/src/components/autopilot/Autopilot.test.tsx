import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AutopilotProgressDto, AutopilotStatusDto } from '../../ipc/types';
import { Autopilot } from './Autopilot';

/**
 * PHASE-28. Every test here is about **the one sentence this panel exists to be honest about**.
 *
 * Zero-Touch does not mean AURA does everything and asks nothing. It means AURA works unattended
 * where phase 13's bands allow, and on this build nothing has been calibrated, so every band is
 * raised one step toward review. The concrete consequence - the product does the work and queues
 * more of it than it eventually will - has to be on the screen *before* the run, not inferred two
 * hours later from a review queue with four hundred frames in it.
 *
 * The second thing asserted here is what the panel cannot do. ADR-0058 section 5 lists five
 * absences, and four of them are absences a later contributor could helpfully add back.
 */

function status(overrides: Partial<AutopilotStatusDto> = {}): AutopilotStatusDto {
  return {
    runs: 0,
    latestRun: null,
    status: null,
    stagesEnabled: 25,
    stagesCompleted: 0,
    stagesDegraded: 0,
    completeness: 0,
    zeroTouch: true,
    calibrated: false,
    resourceEvents: 0,
    bytes: 0,
    policyVer: 1,
    orchestratorVer: 1,
    ...overrides,
  };
}

function progress(): AutopilotProgressDto {
  return {
    runId: 'run_1',
    status: 'running',
    stage: 'previews',
    stageTitle: 'Building previews',
    stageIndex: 1,
    stageTotal: 25,
    itemsDone: 10,
    itemsTotal: 100,
    etaS: 60,
    throughputPerS: 1,
    spendUsd: 0,
    warnings: [],
    currentImage: null,
    cancelled: false,
  };
}

function props(overrides: Partial<Parameters<typeof Autopilot>[0]> = {}) {
  return {
    status: status(),
    stages: [],
    progress: null,
    summary: null,
    events: [],
    preflight: null,
    disabled: [],
    zeroTouch: true,
    onPreflight: vi.fn(),
    onClosePreflight: vi.fn(),
    onStart: vi.fn(),
    onCancel: vi.fn(),
    onToggleStage: vi.fn(),
    onZeroTouch: vi.fn(),
    ...overrides,
  };
}

describe('Autopilot', () => {
  it('says what an uncalibrated build will do before the run starts', () => {
    render(<Autopilot {...props()} />);
    expect(screen.getByText(/has not yet learned how often it is right/)).toBeTruthy();
    expect(screen.getByText(/will not do anything it cannot take back without asking/)).toBeTruthy();
  });

  it('stops saying it once the build is calibrated', () => {
    render(<Autopilot {...props({ status: status({ calibrated: true }) })} />);
    expect(screen.queryByText(/has not yet learned how often it is right/)).toBeNull();
  });

  it('offers exactly one control over how much AURA may do on its own', () => {
    // ADR-0058 section 5: the only autonomy field on this surface is a boolean, and what it unlocks
    // is decided by phase 13's bands. A control that could name a band would be a control that
    // routed around them.
    const { container } = render(<Autopilot {...props()} />);
    const mode = container.querySelector('.autopilot-mode');
    expect(mode?.querySelectorAll('input')).toHaveLength(1);
    expect(mode?.querySelectorAll('select, [type="range"], [type="number"]')).toHaveLength(0);
  });

  it('reports a Zero-Touch change rather than deciding it', () => {
    const onZeroTouch = vi.fn();
    render(<Autopilot {...props({ onZeroTouch })} />);
    fireEvent.click(screen.getByLabelText(/Zero-Touch/));
    expect(onZeroTouch).toHaveBeenCalledWith(false);
  });

  it('does not start a run without going through the pre-flight', () => {
    // The start button opens the checks. Nothing on this panel calls `onStart` directly, because a
    // two-hour job that begins before the disk has been looked at is the failure section 2.1 exists
    // to prevent.
    const onStart = vi.fn();
    const onPreflight = vi.fn();
    render(<Autopilot {...props({ onStart, onPreflight })} />);
    // By role: the heading and the button deliberately carry the same words, which is the product
    // copy rather than an accident, so text alone is ambiguous here.
    fireEvent.click(screen.getByRole('button', { name: 'Edit complete wedding' }));
    expect(onPreflight).toHaveBeenCalledOnce();
    expect(onStart).not.toHaveBeenCalled();
  });

  it('takes the checklist away while a run is in flight', () => {
    // A stage switched off mid-run would be a plan that no longer describes the run being executed.
    const { container } = render(<Autopilot {...props({ progress: progress() })} />);
    expect(container.querySelectorAll('.autopilot-stage input')).toHaveLength(0);
    expect(container.querySelector('.autopilot-mode input')?.hasAttribute('disabled')).toBe(true);
  });

  it('hides the last run while a new one is working', () => {
    const { container } = render(<Autopilot {...props({ progress: progress() })} />);
    expect(screen.queryByText('This wedding has not been run yet.')).toBeNull();
    // The stage's words appear twice while it runs - once as the progress heading and once as its
    // row in the checklist - so this asks the progress section rather than the document.
    expect(container.querySelector('.autopilot-progress h3')?.textContent).toBe(
      'Building previews',
    );
  });

  it('says a stopped run will be continued rather than restarted', () => {
    render(<Autopilot {...props({ status: status({ status: 'cancelled' }) })} />);
    expect(screen.getByText(/picks up where it left off/)).toBeTruthy();
  });

  it('shows the pre-flight only once it has been asked for', () => {
    const { rerender } = render(<Autopilot {...props()} />);
    expect(screen.queryByRole('dialog')).toBeNull();
    rerender(
      <Autopilot
        {...props({
          preflight: {
            verdict: 'pass',
            permitsStart: true,
            images: 10,
            estimatedOutputBytes: 1_000_000,
            estimatedMs: 60_000,
            rows: [],
          },
        })}
      />,
    );
    expect(screen.getByRole('dialog')).toBeTruthy();
  });
});
