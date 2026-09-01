import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { QcReportDto, QcStatusDto } from '../../ipc/types';
import { CategoryFilter } from './CategoryFilter';
import { QcReport } from './QcReport';

/**
 * PHASE-27. Every test here is about **not presenting an unrun check as a passed one**.
 *
 * The views' job is not to render numbers - any component can do that. Their job is to make sure a
 * photographer can never mistake "AURA could not look" for "AURA looked and it is fine", and in
 * this build the first is the common case: phase 06's detector finds no faces, phase 18's
 * segmenter is untrained, and most inspections skip on most frames.
 */

function status(overrides: Partial<QcStatusDto> = {}): QcStatusDto {
  return {
    selected: 800,
    checked: 800,
    coverage: 1,
    inspections: 7200,
    inspectionsSkipped: 0,
    completeness: 1,
    open: 0,
    accepted: 0,
    dismissed: 0,
    falseTicketRate: 0,
    replaced: 0,
    rounds: 0,
    plannerCalls: 0,
    byCategory: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    byStatus: [0, 0, 0, 0, 0, 0],
    bytes: 10480,
    thresholdsVer: 1,
    analysisVer: 1,
    detectorTrained: false,
    ...overrides,
  };
}

function report(overrides: Partial<QcReportDto> = {}): QcReportDto {
  return {
    images: 800,
    imagesUnreached: 0,
    complete: true,
    checksRun: 7200,
    skipped: 0,
    byCategory: [
      { category: 'consistency', found: 4, fixed: 3, escalated: 1, skipped: 0 },
      { category: 'skin', found: 0, fixed: 0, escalated: 0, skipped: 800 },
    ],
    found: 4,
    fixed: 3,
    reverted: 1,
    escalated: 1,
    replacements: [],
    plannerCalls: 0,
    cloudUsed: false,
    durationMs: 4200,
    thresholdsVer: 1,
    analysisVer: 1,
    ...overrides,
  };
}

describe('QcReport', () => {
  it('says how much could not be checked before it says what was found', () => {
    render(
      <QcReport
        status={status({ inspections: 3600, inspectionsSkipped: 3600, completeness: 0.5, open: 4 })}
        report={report()}
        running={false}
        onInspect={vi.fn()}
        onRemediate={vi.fn()}
        onExport={vi.fn()}
      />,
    );
    // The headline names the gap rather than the count. A photographer who reads only the first
    // sentence must not come away thinking half a wedding was inspected.
    expect(screen.getByText(/of the checks it wanted to run could not run/i)).toBeTruthy();
    expect(screen.getByText(/not a clean bill of health/i)).toBeTruthy();
  });

  it('never claims a clean gallery while any check was skipped', () => {
    render(
      <QcReport
        status={status({ inspections: 7000, inspectionsSkipped: 200, completeness: 0.97, open: 0 })}
        report={report()}
        running={false}
        onInspect={vi.fn()}
        onRemediate={vi.fn()}
        onExport={vi.fn()}
      />,
    );
    expect(screen.queryByText(/found nothing/i)).toBeNull();
  });

  it('marks a category that found nothing and skipped everything as unavailable', () => {
    render(
      <QcReport
        status={status({ inspectionsSkipped: 800, completeness: 0.9 })}
        report={report()}
        running={false}
        onInspect={vi.fn()}
        onRemediate={vi.fn()}
        onExport={vi.fn()}
      />,
    );
    const skin = screen.getByRole('row', { name: /Skin/ });
    expect(skin.className).toContain('is-unavailable');
    const consistency = screen.getByRole('row', { name: /Matching the room/ });
    expect(consistency.className).not.toContain('is-unavailable');
  });

  it('renders the untrained-detector caveat rather than hiding it', () => {
    render(
      <QcReport
        status={status()}
        report={report()}
        running={false}
        onInspect={vi.fn()}
        onRemediate={vi.fn()}
        onExport={vi.fn()}
      />,
    );
    expect(screen.getByRole('note').textContent).toContain('No defect-detection model ships');
  });

  it('counts a reverted remedy separately from a fix', () => {
    render(
      <QcReport
        status={status()}
        report={report()}
        running={false}
        onInspect={vi.fn()}
        onRemediate={vi.fn()}
        onExport={vi.fn()}
      />,
    );
    // Three fixed and one put back. A single "resolved" number would describe work that is not in
    // the delivered file.
    expect(screen.getByText('Tried and put back').nextElementSibling?.textContent).toBe('1');
    expect(screen.getByText('Fixed and confirmed').nextElementSibling?.textContent).toBe('3');
  });
});

describe('CategoryFilter', () => {
  it('reads a zero as "not checked" when anything was skipped', () => {
    render(
      <CategoryFilter
        status={status({ inspectionsSkipped: 400, byCategory: [3, 0, 0, 0, 0, 0, 0, 0, 0, 0] })}
        selected={null}
        onSelect={vi.fn()}
      />,
    );
    const skin = screen.getByRole('button', { name: /^Skin/ });
    expect(skin.textContent).toContain('not checked');
    expect(skin.className).toContain('is-unavailable');
  });

  it('reads a zero as zero when nothing was skipped', () => {
    render(
      <CategoryFilter
        status={status({ inspectionsSkipped: 0, byCategory: [3, 0, 0, 0, 0, 0, 0, 0, 0, 0] })}
        selected={null}
        onSelect={vi.fn()}
      />,
    );
    const skin = screen.getByRole('button', { name: /^Skin/ });
    expect(skin.textContent).not.toContain('not checked');
    expect(skin.className).not.toContain('is-unavailable');
  });
});
