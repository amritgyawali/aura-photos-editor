import { fireEvent, render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { freshRetouch } from '../../ipc/nativeRetouch';
import { RetouchSkinSelection } from './RetouchSkinSelection';

it('intersects a saved garment matte with the current color range', () => {
  const change=vi.fn();
  const color={sample:[.7,.3] as [number,number],tolerance:15,softness:10,lightness:.5};
  const clothes={...freshRetouch(),id:'clothes',matte:'auto-portrait-v20-0-clothes'};
  render(<RetouchSkinSelection draft={{...freshRetouch(),tool:'color_saturation',selection:{color}}} edits={[clothes]} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('Use detected region'),{target:{value:clothes.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:clothes.matte,selection:{color}}));
});

it('keeps the zoom centre when reusing a detected background selection', () => {
  const change = vi.fn();
  const background = {...freshRetouch(),matte:'auto-portrait-v20-0-background',id:'background',source:[.1,.2] as [number,number]};
  render(<RetouchSkinSelection draft={{...freshRetouch(),tool:'zoom_blur',source:[.3,.4]}} edits={[background]} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('Use detected region'), {target:{value:background.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:background.matte,source:[.3,.4]}));
});

it.each(['fabric_moire','fabric_stain','background_blur','highlight_glow'] as const)('reuses material and background masks for %s', tool => {
  const change = vi.fn();
  const material = {...freshRetouch(), matte:'auto-portrait-v20-0-clothes', id:'cloth', source:[.1,.2] as [number,number]};
  render(<RetouchSkinSelection draft={{...freshRetouch(), tool}} edits={[material]} onChange={change}/>);
  fireEvent.change(screen.getByLabelText('Use detected region'), {target:{value:material.matte}});
  expect(change).toHaveBeenCalledWith(expect.objectContaining({matte:material.matte}));
  expect(change.mock.calls[0]?.[0]).not.toHaveProperty('tool');
  if (tool === 'fabric_stain') expect(change.mock.calls[0]?.[0].source).toBeNull();
});

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
