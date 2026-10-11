import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { freshRetouch } from '../../ipc/nativeRetouch';
import { RetouchSkinSelection } from './RetouchSkinSelection';

it('reuses the hair selection for color without copying an automatic repair', () => {
  const change = vi.fn();
  const hair = {...freshRetouch(),matte:'auto-portrait-v20-0-hair',id:'hair'};
  render(<RetouchSkinSelection draft={{...freshRetouch(),tool:'colorize',targetColor:[1,0,0]}} edits={[hair]} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('Use detected region'), {target:{value:hair.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:hair.matte}));
  expect(change.mock.calls[0]?.[0]).not.toHaveProperty('tool');
  expect(change.mock.calls[0]?.[0]).not.toHaveProperty('targetColor');
});

it('offers the nose-inclusive blemish selection for the correct person',()=>{
  const edit={...freshRetouch(),id:'clear',matte:'auto-portrait-v1-2-blemish-surface-blemish-feature-safe'};
  const change=vi.fn();
  render(<RetouchSkinSelection draft={freshRetouch()} edits={[edit]} onChange={change}/>);
  expect(screen.getByRole('option',{name:/Blemish cleanup skin.*Person 3.*protected details/})).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Use detected skin'),{target:{value:edit.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:edit.matte}));
});

it('reuses body skin while retaining exclusions for all faces',()=>{
  const edit={...freshRetouch(),id:'body-spots',matte:'auto-portrait-v1-1-body-outside-faces'};
  const change=vi.fn();
  render(<RetouchSkinSelection draft={freshRetouch()} edits={[edit]} onChange={change}/>);
  expect(screen.getByRole('option',{name:/Body skin.*Person 2.*protected details/})).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Use detected skin'),{target:{value:edit.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:edit.matte}));
});

it('reuses the protected cleanup matte without dropping eye and nose exclusions',()=>{
  const edit={...freshRetouch(),id:'clear',matte:'auto-portrait-v1-0-surface-feature-safe'};
  const change=vi.fn();
  render(<RetouchSkinSelection draft={freshRetouch()} edits={[edit]} onChange={change}/>);
  expect(screen.getByRole('option',{name:/Blemish cleanup skin.*protected details/})).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Use detected skin'),{target:{value:edit.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:edit.matte}));
});

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
