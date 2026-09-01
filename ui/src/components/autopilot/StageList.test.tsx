import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { AutopilotStageDto } from '../../ipc/types';
import { PLAN, StageList } from './StageList';

/**
 * PHASE-28. Every test here is about **a skipped step never looking like a finished one**.
 *
 * A step that did not run means several completely different things - the photographer switched it
 * off, this release does not have it, its model is untrained, or AURA is not confident enough to
 * act unattended - and only the first of those is fine. Phase 27 asserted the same distinction for
 * an inspection; this is it one level up, where the subject is a whole step and the reader is
 * somebody who has come back two hours later.
 */

function stage(overrides: Partial<AutopilotStageDto> = {}): AutopilotStageDto {
  return {
    stage: 'retouch',
    title: 'Retouching skin',
    outcome: 'completed',
    skipCause: null,
    skipText: null,
    verdict: 'act',
    itemsDone: 40,
    itemsTotal: 40,
    elapsedMs: 1200,
    attempts: 1,
    reasons: [],
    ...overrides,
  };
}

describe('StageList', () => {
  it('renders the whole plan before a run, so the checklist is the wedding', () => {
    render(<StageList stages={[]} disabled={[]} />);
    const rows = screen.getAllByRole('listitem');
    expect(rows).toHaveLength(PLAN.length);
    expect(rows).toHaveLength(25);
  });

  it('never renders a step that could not run as a finished one', () => {
    const causes = [
      'phase_not_built',
      'service_absent',
      'model_untrained',
      'no_input',
      'awaiting_review',
      'resource_stopped',
      'cancelled',
    ];
    for (const cause of causes) {
      const { container, unmount } = render(
        <StageList
          stages={[stage({ outcome: 'skipped', skipCause: cause, skipText: 'because' })]}
          disabled={[]}
        />,
      );
      const row = container.querySelector('[data-stage="retouch"]');
      expect(row?.className, cause).toContain('is-degraded');
      expect(row?.className, cause).not.toContain('is-done');
      unmount();
    }
  });

  it('does not degrade a step the photographer switched off', () => {
    // The one skip cause that is fine, and the whole reason `skipCause` is on the wire beside
    // `outcome` rather than folded into it.
    const { container } = render(
      <StageList
        stages={[stage({ outcome: 'skipped', skipCause: 'turned_off', skipText: 'You turned this off' })]}
        disabled={['retouch']}
      />,
    );
    const row = container.querySelector('[data-stage="retouch"]');
    expect(row?.className).not.toContain('is-degraded');
    expect(row?.className).toContain('is-off');
  });

  it('shows the sentence the orchestrator gave for a skip rather than inventing one', () => {
    render(
      <StageList
        stages={[
          stage({
            outcome: 'skipped',
            skipCause: 'phase_not_built',
            skipText: 'This release does not include this step yet',
          }),
        ]}
        disabled={[]}
      />,
    );
    expect(
      screen.getByText('This release does not include this step yet'),
    ).toBeTruthy();
  });

  it('marks a step whose decisions go in the review queue', () => {
    render(<StageList stages={[stage({ verdict: 'act_and_review' })]} disabled={[]} />);
    expect(screen.getByText('worth a look')).toBeTruthy();
  });

  it('offers no switch for a step a wedding cannot be delivered without', () => {
    // A control that looks like a control and does nothing is worse than no control. The four
    // mandatory stages have no checkbox at all rather than a disabled one.
    const onToggle = vi.fn();
    const { container } = render(
      <StageList stages={[]} disabled={[]} onToggle={onToggle} />,
    );
    for (const slug of ['ingest', 'previews', 'embed', 'cull']) {
      const row = container.querySelector(`[data-stage="${slug}"]`);
      expect(row?.querySelector('input'), slug).toBeNull();
    }
    expect(container.querySelector('[data-stage="retouch"] input')).not.toBeNull();
  });

  it('reports a toggle by slug rather than by label', () => {
    const onToggle = vi.fn();
    render(<StageList stages={[]} disabled={[]} onToggle={onToggle} />);
    fireEvent.click(screen.getByLabelText('Retouching skin'));
    expect(onToggle).toHaveBeenCalledWith('retouch', false);
  });

  it('offers no switches at all while a run is in flight', () => {
    const { container } = render(<StageList stages={[]} disabled={[]} />);
    expect(container.querySelectorAll('input')).toHaveLength(0);
  });

  it('shows a partial step as part done rather than as done', () => {
    render(
      <StageList
        stages={[stage({ outcome: 'partial', itemsDone: 18, itemsTotal: 40 })]}
        disabled={[]}
      />,
    );
    expect(screen.getByText('18 of 40 done')).toBeTruthy();
  });
});
