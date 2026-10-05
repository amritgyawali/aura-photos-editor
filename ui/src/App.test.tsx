import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { App } from './App';
afterEach(cleanup);
it('always opens the screenshot studio shell even with a saved alternate theme', () => {
  localStorage.setItem('aura.theme', 'light');
  const first = render(<App />);
  const nav = screen.getByRole('navigation', { name: 'Workspace' });
  expect(within(nav).getAllByRole('button').map(button => button.querySelector('strong')?.textContent))
    .toEqual(['Start', 'Photos', 'Auto edit', 'Instagram style', 'Export', 'Advanced']);
  expect(screen.queryByRole('button', { name: /Theme:/ })).toBeNull();
  fireEvent.click(within(nav).getByRole('button', { name: /Auto edit/ }));
  expect(first.container.querySelector('.aura-studio')).toBeTruthy();
  first.unmount();
  const second = render(<App />);
  expect(second.container.querySelector('.aura-studio')).toBeTruthy();
  expect(screen.getByRole('button', { name: /Start Look/ }).getAttribute('aria-current')).toBe('page');
  localStorage.removeItem('aura.theme');
});
