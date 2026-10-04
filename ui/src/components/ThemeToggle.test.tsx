import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { ThemeToggle } from './ThemeToggle';

describe('ThemeToggle', () => {
  beforeEach(() => {
    window.localStorage.clear();
    document.documentElement.removeAttribute('data-theme');
  });

  it('starts on the system default and stamps nothing on the root', () => {
    render(<ThemeToggle />);
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(screen.getByRole('button', { name: '◐ Auto' })).toBeDefined();
  });

  it('cycles auto to dark to light and stamps the root as it goes', () => {
    render(<ThemeToggle />);
    const button = screen.getByRole('button');
    fireEvent.click(button);
    expect(document.documentElement.dataset.theme).toBe('dark');
    fireEvent.click(button);
    expect(document.documentElement.dataset.theme).toBe('light');
    fireEvent.click(button);
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(window.localStorage.getItem('aura.theme')).toBeNull();
  });

  it('keeps an explicit choice across a remount', () => {
    const first = render(<ThemeToggle />);
    fireEvent.click(screen.getByRole('button'));
    first.unmount();
    render(<ThemeToggle />);
    expect(screen.getByRole('button', { name: '☾ Dark' })).toBeDefined();
    expect(document.documentElement.dataset.theme).toBe('dark');
  });
});
