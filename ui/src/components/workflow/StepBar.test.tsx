import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { STEPS, STEPS_BY_ID, stepOf } from './steps';
import { StepBar } from './StepBar';
import type { StepBarRow } from './StepBar';
import type { StepEvidence } from '../../state/workflowStore';

function row(index: number, evidence: Partial<StepEvidence> = {}): StepBarRow {
  return {
    step: STEPS_BY_ID[STEPS[index]?.id ?? 'import'],
    evidence: { status: 'todo', hint: '', fetched: false, ...evidence },
    isHere: false,
  };
}

const ALL = STEPS.map((_, index) => row(index));

describe('StepBar', () => {
  it('offers every step of a wedding', () => {
    render(<StepBar rows={ALL} onGo={() => undefined} />);
    for (const step of STEPS) {
      expect(screen.getByRole('button', { name: new RegExp(step.title) })).toBeDefined();
    }
  });

  it('asks for the step it was given', () => {
    const onGo = vi.fn();
    render(<StepBar rows={ALL} onGo={onGo} />);
    fireEvent.click(screen.getByRole('button', { name: /Cull/ }));
    expect(onGo).toHaveBeenCalledWith(stepOf('cull'));
  });

  it('says what is waiting in words, not only in color', () => {
    // A dot is the glance; the word is the screen reader's and the bad panel's.
    // "Needs you" is also the one state the bar exists to make unmissable.
    const rows = STEPS.map((_, index) =>
      row(index, index === 2 ? { status: 'warn', hint: 'A pass has never run.' } : {}),
    );
    render(<StepBar rows={rows} onGo={() => undefined} />);
    const cull = screen.getByRole('button', { name: /Cull/ });
    expect(cull.textContent).toContain('Needs you');
    expect(cull.textContent).toContain('A pass has never run.');
  });

  it('falls back to the purpose when an evidence line has nothing to say', () => {
    render(<StepBar rows={ALL} onGo={() => undefined} />);
    expect(screen.getByRole('button', { name: /Export/ }).textContent).toContain(
      'One press: edit everything and deliver the files.',
    );
  });

  it('marks where the photographer actually is', () => {
    const rows = ALL.map((entry, index) => ({ ...entry, isHere: index === 3 }));
    const { container } = render(<StepBar rows={rows} onGo={() => undefined} />);
    const items = container.querySelectorAll('.step-bar-item');
    expect(items[3]?.classList.contains('is-here')).toBe(true);
    expect(items[0]?.classList.contains('is-here')).toBe(false);
  });
});
