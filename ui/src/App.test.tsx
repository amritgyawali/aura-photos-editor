import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { App } from './App';
afterEach(cleanup);
it('always opens the screenshot studio shell even with a saved alternate theme', () => {
  localStorage.setItem('aura.theme', 'light');
  const first = render(<App />);
  const nav = screen.getByRole('navigation', { name: 'Workspace' });
  expect(within(nav).getAllByRole('button').map(button => button.querySelector('strong')?.textContent))
    .toEqual(['Start', 'Photos', 'Cull', 'Auto edit', 'Effects', 'Match a look', 'Hot Features', 'Export', 'Advanced']);
  expect(screen.queryByRole('button', { name: /Theme:/ })).toBeNull();
  fireEvent.click(within(nav).getByRole('button', { name: /Auto edit/ }));
  expect(first.container.querySelector('.aura-studio')).toBeTruthy();
  first.unmount();
  const second = render(<App />);
  expect(second.container.querySelector('.aura-studio')).toBeTruthy();
  expect(screen.getByRole('button', { name: /Start Look/ }).getAttribute('aria-current')).toBe('page');
  localStorage.removeItem('aura.theme');
});

it('opens all five Hot Features from the sidebar before importing photos', () => {
  render(<App />);
  fireEvent.click(within(screen.getByRole('navigation', { name: 'Workspace' })).getByRole('button', { name: /Hot Features/ }));
  const section = screen.getByRole('region', { name: 'Hot Features' });
  for (const title of ['Liquify Background Repair', 'Face Freckles & Acne', 'Remove Glasses Glare', 'Train Your AI Look', 'AI Color Match']) {
    expect(within(section).getByRole('button', { name: title })).toBeTruthy();
  }
  fireEvent.click(within(section).getByRole('button', { name: 'AI Color Match' }));
  expect(within(section).getByRole('button', { name: 'AI Color Match' }).getAttribute('aria-pressed')).toBe('true');
  expect(screen.getByRole('button', { name: 'Add photos for Hot Features' })).toBeTruthy();
});
