import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { freshRetouch } from '../../ipc/nativeRetouch';
import { RetouchPresets, parseRetouchPresets } from './RetouchPresets';

beforeEach(() => localStorage.clear());
it('persists reusable settings without carrying masks, sources or photo-specific IDs', () => {
  const apply = vi.fn();
  const draft = { ...freshRetouch(), tool: 'dodge' as const, source: [.2,.3] as [number, number], mask: { strokes: [] } };
  const first = render(<RetouchPresets draft={draft} disabled={false} onApply={apply}/>);
  fireEvent.change(screen.getByLabelText('Preset name'), { target: { value: 'Soft dodge' } });
  fireEvent.click(screen.getByText('Save tool preset')); first.unmount();
  render(<RetouchPresets draft={freshRetouch()} disabled={false} onApply={apply}/>);
  fireEvent.change(screen.getByLabelText('Saved preset'), { target: { value: 'Soft dodge' } });
  fireEvent.click(screen.getByText('Load tool preset'));
  expect(apply).toHaveBeenCalledWith(expect.objectContaining({ tool: 'dodge', amount: .65 }));
  expect(apply.mock.calls[0]?.[0]).not.toHaveProperty('mask');
  expect(apply.mock.calls[0]?.[0]).not.toHaveProperty('source');
  expect(apply.mock.calls[0]?.[0]).not.toHaveProperty('id');
});
it('rejects corrupt storage and surfaces storage write failures', () => {
  expect(() => parseRetouchPresets('{"version":2,"items":[]}')).toThrow('unsupported');
  const spy = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Storage is full'); });
  render(<RetouchPresets draft={freshRetouch()} disabled={false} onApply={vi.fn()}/>);
  fireEvent.change(screen.getByLabelText('Preset name'), { target: { value: 'My preset' } });
  fireEvent.click(screen.getByText('Save tool preset'));
  expect(screen.getByRole('alert').textContent).toContain('Storage is full');
  spy.mockRestore();
});
it('round-trips skin controls, strips extra sample data and rejects an invalid range', () => {
  const payload={version:1,items:[{name:'Portrait',settings:{...freshRetouch(),tool:'skin_smooth',
    skin:{tolerance:.12,edgeProtection:.85,source:[.2,.3]}}}]};
  const result=parseRetouchPresets(JSON.stringify(payload));
  expect(result[0]?.settings.skin).toEqual({tolerance:.12,edgeProtection:.85});
  expect(result[0]?.settings).not.toHaveProperty('source');
  payload.items[0]!.settings.skin.tolerance=.9;
  expect(()=>parseRetouchPresets(JSON.stringify(payload))).toThrow('invalid');
});
