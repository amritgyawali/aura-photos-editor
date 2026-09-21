import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { OnboardingOverlay, tourIsOpenable } from './OnboardingOverlay';

describe('OnboardingOverlay', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('is offered before anybody has seen it', () => {
    expect(tourIsOpenable()).toBe(true);
  });

  it('walks from the first step to the last', () => {
    render(<OnboardingOverlay onDismiss={() => undefined} />);
    expect(screen.getByRole('heading', { name: 'Import' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Back' }).hasAttribute('disabled')).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    expect(screen.getByRole('heading', { name: 'Analyze' })).toBeDefined();

    for (const _ of [0, 1, 2]) {
      fireEvent.click(screen.getByRole('button', { name: 'Next' }));
    }
    expect(screen.getByRole('heading', { name: 'Export' })).toBeDefined();
    expect(screen.getByRole('button', { name: 'Start working' })).toBeDefined();
  });

  it('dismisses on skip, and never offers itself again afterwards', () => {
    const onDismiss = vi.fn();
    render(<OnboardingOverlay onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole('button', { name: 'Skip the tour' }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    expect(tourIsOpenable()).toBe(false);
  });
});
