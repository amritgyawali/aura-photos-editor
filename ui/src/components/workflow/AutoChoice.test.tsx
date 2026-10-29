import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { AUTO_CHOICE_MS, AutoChoiceNotice, clock, useAutoChoice } from './AutoChoice';

function Step({ armed, choose, step = 1 }: { armed: boolean; choose: () => void; step?: number }) {
  const choice = useAutoChoice(armed, choose, step);
  return <AutoChoiceNotice choice={choice} action="edit the 3 best photos" />;
}

beforeEach(() => { vi.useFakeTimers(); });
afterEach(() => { cleanup(); vi.useRealTimers(); });

it('takes the recommended choice after one minute with nobody there, once', () => {
  const choose = vi.fn();
  render(<Step armed choose={choose} />);
  expect(AUTO_CHOICE_MS).toBe(60_000);
  expect(screen.getByRole('status').textContent).toContain('AURA will edit the 3 best photos in 1:00');
  act(() => { vi.advanceTimersByTime(30_000); });
  expect(screen.getByRole('status').textContent).toContain('in 0:30');
  expect(choose).not.toHaveBeenCalled();
  act(() => { vi.advanceTimersByTime(30_500); });
  expect(choose).toHaveBeenCalledOnce();
  expect(screen.queryByRole('status')).toBeNull();
  act(() => { vi.advanceTimersByTime(180_000); });
  expect(choose).toHaveBeenCalledOnce();
});

it('starts the minute again on any mouse or key activity', () => {
  const choose = vi.fn();
  render(<Step armed choose={choose} />);
  act(() => { vi.advanceTimersByTime(50_000); });
  fireEvent.pointerMove(window);
  act(() => { vi.advanceTimersByTime(50_000); });
  fireEvent.keyDown(window, { key: 'a' });
  act(() => { vi.advanceTimersByTime(50_000); });
  expect(choose).not.toHaveBeenCalled();
  expect(screen.getByRole('status').textContent).toContain('in 0:10');
  act(() => { vi.advanceTimersByTime(10_500); });
  expect(choose).toHaveBeenCalledOnce();
});

it('never runs while not armed, and stops for good when the person will decide', () => {
  const choose = vi.fn();
  const { rerender } = render(<Step armed={false} choose={choose} />);
  expect(screen.queryByRole('status')).toBeNull();
  act(() => { vi.advanceTimersByTime(120_000); });
  expect(choose).not.toHaveBeenCalled();
  rerender(<Step armed choose={choose} />);
  fireEvent.click(screen.getByRole('button', { name: 'Let me decide' }));
  expect(screen.queryByRole('status')).toBeNull();
  act(() => { vi.advanceTimersByTime(120_000); });
  expect(choose).not.toHaveBeenCalled();
  // A new step is a new decision: its minute runs again.
  rerender(<Step armed choose={choose} step={2} />);
  act(() => { vi.advanceTimersByTime(60_500); });
  expect(choose).toHaveBeenCalledOnce();
});

it('reads the time left as minutes and seconds', () => {
  expect(clock(60)).toBe('1:00');
  expect(clock(42)).toBe('0:42');
  expect(clock(5)).toBe('0:05');
});
