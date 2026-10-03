import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { freshRetouch } from '../../ipc/nativeRetouch';
import { RetouchSkinSelection } from './RetouchSkinSelection';

it('reuses body coverage and sample without copying the saved tool or strength', () => {
  const body = { ...freshRetouch(), id: 'auto-portrait-v1-1-body-texture',
    matte: 'auto-portrait-v1-1-body', source: [.6, .7] as [number, number],
    mask: { strokes: [{ erase: false, radius: .1, opacity: 1, points: [[.6, .7, 1] as [number, number, number]] }] } };
  const change = vi.fn();
  render(<RetouchSkinSelection draft={freshRetouch()} edits={[body, { ...body, id: 'tone' }]} onChange={change}/>);
  expect(screen.getAllByRole('option', { name: 'Body skin · Person 2' })).toHaveLength(1);
  fireEvent.change(screen.getByLabelText('Use detected skin'), { target: { value: body.matte } });
  expect(change).toHaveBeenCalledWith({ matte: body.matte, mask: body.mask, region: body.region,
    source: body.source, skin: null, selection: null, feather: body.feather });
});

it('exposes and removes an attached mask even when it is a protected manual snapshot', () => {
  const change = vi.fn();
  render(<RetouchSkinSelection draft={{ ...freshRetouch(), matte: 'manual-snapshot' }} edits={[]} onChange={change}/>);
  expect(screen.getByText(/AI mask active/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Remove AI mask restriction' }));
  expect(change).toHaveBeenCalledWith({ matte: null });
});
