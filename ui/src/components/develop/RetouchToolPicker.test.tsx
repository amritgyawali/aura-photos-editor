import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RetouchToolPicker } from './RetouchToolPicker';

describe('retouch tool search', () => {
  it('finds material tools by purpose and preserves the current selection while filtering', () => {
    const change = vi.fn();
    render(<RetouchToolPicker value="heal" onChange={change} />);
    fireEvent.change(screen.getByLabelText('Find a retouch tool'), { target: { value: 'sweat stain' } });
    expect(screen.getByRole('option', { name: 'Fabric stain correction' })).toBeTruthy();
    expect((screen.getByLabelText('Tool') as HTMLSelectElement).value).toBe('heal');
    expect(change).not.toHaveBeenCalled();
    fireEvent.change(screen.getByLabelText('Tool'), { target: { value: 'fabric_stain' } });
    expect(change).toHaveBeenCalledWith('fabric_stain');
  });

  it('makes no-result searches recoverable without changing the authored tool', () => {
    render(<RetouchToolPicker value="shadow_balance" onChange={vi.fn()} />);
    fireEvent.change(screen.getByLabelText('Find a retouch tool'), { target: { value: 'unavailable feature' } });
    expect(screen.getByRole('status').textContent).toContain('No matching tools');
    expect((screen.getByLabelText('Tool') as HTMLSelectElement).value).toBe('shadow_balance');
    fireEvent.click(screen.getByRole('button', { name: 'Clear tool search' }));
    expect(screen.getByRole('option', { name: 'Highlight glow' })).toBeTruthy();
  });
});
