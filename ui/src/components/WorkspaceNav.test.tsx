import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { WORKSPACES, WorkspaceNav } from './WorkspaceNav';

describe('WorkspaceNav', () => {
  it('offers every workspace', () => {
    render(<WorkspaceNav active="library" onSelect={() => undefined} />);
    for (const workspace of WORKSPACES) {
      expect(screen.getByRole('button', { name: workspace.title })).toBeDefined();
    }
  });

  it('marks the open one as the current page rather than only styling it', () => {
    render(<WorkspaceNav active="develop" onSelect={() => undefined} />);
    const develop = screen.getByRole('button', { name: 'Develop' });
    expect(develop.getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('button', { name: 'Library' }).getAttribute('aria-current')).toBe(
      null,
    );
  });

  it('switches on a click', () => {
    const onSelect = vi.fn();
    render(<WorkspaceNav active="library" onSelect={onSelect} />);
    fireEvent.click(screen.getByRole('button', { name: 'Cull' }));
    expect(onSelect).toHaveBeenCalledWith('cull');
  });

  it('leaves the library reachable with no wedding open, and nothing else', () => {
    // Every other workspace is about a project. Disabling them is the honest state: a cull view
    // with no wedding behind it would render an empty gallery, which reads as "nothing was
    // selected" rather than as "nothing has been opened".
    render(<WorkspaceNav active="library" onSelect={() => undefined} disabled />);
    expect(screen.getByRole('button', { name: 'Library' }).hasAttribute('disabled')).toBe(false);
    for (const workspace of WORKSPACES.filter((row) => row.id !== 'library')) {
      expect(
        screen.getByRole('button', { name: workspace.title }).hasAttribute('disabled'),
        `${workspace.title} should need a wedding`,
      ).toBe(true);
    }
  });

  it('says what each workspace is for, so the labels do not have to carry it', () => {
    render(<WorkspaceNav active="library" onSelect={() => undefined} />);
    expect(screen.getByRole('button', { name: 'Cull' }).getAttribute('title')).toBe(
      'What is being delivered, and why.',
    );
  });

  it('lists the workspaces in the order the work happens in', () => {
    // Not decoration: the order below is the order the autopilot's DAG runs its stages, and a
    // navigation that ran backwards would teach a photographer the wrong order of operations.
    expect(WORKSPACES.map((row) => row.id)).toEqual([
      'library',
      'people',
      'story',
      'moments',
      'cull',
      'develop',
      'cleanup',
      'style',
      'camera',
    ]);
  });
});
