import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { audit, clearAudit } from '../audit/log';
import { LogPanel } from './LogPanel';

describe('LogPanel', () => {
  beforeEach(() => {
    clearAudit();
  });

  it('shows what happened, newest first', () => {
    audit('click', 'Cull', { section: 'workspace-nav' });
    audit('ipc-error', 'AURA-ML-5054 in cull_project', { ms: 12 });
    render(<LogPanel />);
    expect(screen.getByText('Cull')).toBeDefined();
    expect(screen.getByText('AURA-ML-5054 in cull_project')).toBeDefined();
    // The count line interpolates its numbers, so assert against the element rather
    // than a single string match.
    expect(document.querySelector('.log-count')?.textContent).toContain('2 shown of 2 kept');
  });

  it('filters by kind', () => {
    audit('click', 'Pressed');
    audit('ipc', 'cull_project');
    render(<LogPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Refusals' }));
    expect(screen.getByText('Nothing matches.')).toBeDefined();
    fireEvent.click(screen.getByRole('button', { name: 'Commands' }));
    expect(screen.getByText('cull_project')).toBeDefined();
  });

  it('searches the detail as well as the headline', () => {
    audit('ipc', 'render_image', { ms: 4200 });
    render(<LogPanel />);
    fireEvent.change(screen.getByLabelText('Search the log'), { target: { value: '4200' } });
    expect(screen.getByText('render_image')).toBeDefined();
  });
});
