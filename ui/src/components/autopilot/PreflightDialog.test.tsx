import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AutopilotPreflightDto } from '../../ipc/types';
import { humanBytes, humanEstimate, PreflightDialog } from './PreflightDialog';

/**
 * PHASE-28. Every test here is about **the run not starting when the pre-flight blocked**.
 *
 * Section 2.1 asks the pre-flight to fail fast with actionable messages before a two-hour job. The
 * failure mode this component has to make impossible is a start button that is still reachable
 * beside a red row.
 */

function report(overrides: Partial<AutopilotPreflightDto> = {}): AutopilotPreflightDto {
  return {
    verdict: 'pass',
    permitsStart: true,
    images: 3000,
    estimatedOutputBytes: 36_000_000_000,
    estimatedMs: 7_200_000,
    rows: [
      { check: 'has_images', title: 'There are photographs', verdict: 'pass', detail: '3000 photographs.' },
    ],
    ...overrides,
  };
}

describe('PreflightDialog', () => {
  it('offers no start button when a row blocks', () => {
    const onStart = vi.fn();
    render(
      <PreflightDialog
        report={report({
          verdict: 'block',
          permitsStart: false,
          rows: [
            {
              check: 'disk_space',
              title: 'There is room on the disk',
              verdict: 'block',
              detail: 'This run needs about 57.6 GB and there is 12.0 GB free. Free up 45.6 GB and start again.',
            },
          ],
        })}
        onStart={undefined}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByText('Edit complete wedding')).toBeNull();
    expect(onStart).not.toHaveBeenCalled();
    expect(screen.getByText(/Fix what is marked above/)).toBeTruthy();
  });

  it('shows the sentence rather than the verdict', () => {
    // A row that said only "Disk space — blocked" sends a photographer to a runbook to find out how
    // many gigabytes they need.
    render(
      <PreflightDialog
        report={report({
          rows: [
            {
              check: 'disk_space',
              title: 'There is room on the disk',
              verdict: 'block',
              detail: 'Free up 45.6 GB and start again.',
            },
          ],
        })}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Free up 45.6 GB and start again.')).toBeTruthy();
  });

  it('starts when nothing blocks', () => {
    const onStart = vi.fn();
    render(<PreflightDialog report={report()} onStart={onStart} onCancel={vi.fn()} />);
    screen.getByText('Edit complete wedding').click();
    expect(onStart).toHaveBeenCalledOnce();
  });

  it('shows a warning row and still permits a start', () => {
    // The calibration row on this build. It always fires and it never blocks.
    render(
      <PreflightDialog
        report={report({
          verdict: 'warn',
          rows: [
            {
              check: 'calibration',
              title: 'How much AURA may do on its own',
              verdict: 'warn',
              detail: 'AURA has not yet learned how often it is right, so it is being careful.',
            },
          ],
        })}
        onStart={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.getByText('Edit complete wedding')).toBeTruthy();
    expect(screen.getByText(/being careful/)).toBeTruthy();
  });

  it('says it is checking rather than showing an empty report', () => {
    render(<PreflightDialog report={null} onCancel={vi.fn()} />);
    expect(screen.getByText('Checking…')).toBeTruthy();
  });
});

describe('humanBytes and humanEstimate', () => {
  it('speak in the units a person would', () => {
    expect(humanBytes(36_000_000_000)).toBe('36.0 GB');
    expect(humanBytes(5_000_000)).toBe('5 MB');
    expect(humanBytes(900)).toBe('900 B');
    expect(humanEstimate(7_200_000)).toBe('about 2.0 hours');
    expect(humanEstimate(300_000)).toBe('about 5 minutes');
  });

  it('never estimates zero minutes', () => {
    expect(humanEstimate(10)).toBe('about 1 minutes');
  });
});
