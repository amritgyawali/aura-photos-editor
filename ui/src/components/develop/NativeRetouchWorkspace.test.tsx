import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { NativeRetouchWorkspace } from './NativeRetouchWorkspace';
import { nativeRetouch, freshRetouch } from '../../ipc/nativeRetouch';
import { develop } from '../../ipc/client';

vi.mock('../../ipc/nativeRetouch',async()=>({...await vi.importActual('../../ipc/nativeRetouch'),nativeRetouch:{edit:vi.fn(),preview:vi.fn(),draftPreview:vi.fn()}}));
vi.mock('../../ipc/client',()=>({asIpcError:(e:Error)=>({message:e.message}),develop:{imageHistory:vi.fn(),historyStep:vi.fn()}}));
beforeEach(()=>{
  vi.resetAllMocks();vi.mocked(nativeRetouch.edit).mockResolvedValue([]);
  vi.mocked(nativeRetouch.preview).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(120,100,90)),notes:[]} as never);
  vi.mocked(develop.imageHistory).mockResolvedValue({canUndo:true,canRedo:false} as never);
  localStorage.clear();
});
function open(){return render(<NativeRetouchWorkspace projectId="project" photoId="photo" onClose={vi.fn()} onBusyChange={vi.fn()}/>);}
it('allows automatic small patch repairs and requires a source for larger or painted repairs',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'patch_heal'}});
  const apply=screen.getByText('Apply retouch') as HTMLButtonElement;
  expect(apply.disabled).toBe(false);
  fireEvent.change(screen.getByLabelText('Horizontal radius (%)'),{target:{value:'11'}});
  expect(apply.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('Horizontal radius (%)'),{target:{value:'3'}});
  expect(apply.disabled).toBe(false);
  fireEvent.click(screen.getByText('Brush (B)'));
  fireEvent.click(screen.getByText('Dab at target coordinates'));
  expect(apply.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('Source X (%)'),{target:{value:'40'}});
  fireEvent.change(screen.getByLabelText('Source Y (%)'),{target:{value:'35'}});
  fireEvent.click(apply);
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[
    expect.objectContaining({tool:'patch_heal',source:[.4,.35],mask:{strokes:[expect.objectContaining({erase:false})]}})
  ]));
});
it('blocks changes during collection editing and refreshes after external revisions',async()=>{
  const props={projectId:'project',photoId:'photo',onClose:vi.fn(),onBusyChange:vi.fn()};
  const view=render(<NativeRetouchWorkspace {...props} disabled revision={0}/>);
  await screen.findByAltText('Retouched photograph');
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).matches(':disabled')).toBe(true);
  view.rerender(<NativeRetouchWorkspace {...props} disabled={false} revision={1}/>);
  await waitFor(()=>expect(nativeRetouch.preview).toHaveBeenCalledTimes(4));
  await waitFor(()=>expect((screen.getByText('Apply retouch') as HTMLButtonElement).matches(':disabled')).toBe(false));
});
it('requires a sampled source for cloning and saves normalized target coordinates',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'clone'}});
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('Source X (%)'),{target:{value:'20'}});
  fireEvent.change(screen.getByLabelText('Source Y (%)'),{target:{value:'30'}});
  fireEvent.change(screen.getByLabelText('Center X (%)'),{target:{value:'60'}});
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[expect.objectContaining({tool:'clone',source:[0.2,0.3],region:[0.6,0.45,0.035,0.035]})]));
});
it('applies a three-operation skin preset in one history action',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByText('Natural skin in selection'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[expect.objectContaining({tool:'frequency',texture:1}),expect.objectContaining({tool:'micro_dodge_burn'}),expect.objectContaining({tool:'mattify'})]));
});
it('updates an existing operation without adding a duplicate',async()=>{
  const saved={...freshRetouch(),id:'saved',tool:'frequency' as const};vi.mocked(nativeRetouch.edit).mockResolvedValue([saved]);
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByText('1. Frequency separation · 65%'));
  fireEvent.change(screen.getByLabelText('Texture gain (100%)'),{target:{value:'1.2'}});
  fireEvent.click(screen.getByText('Update selected retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','update',[expect.objectContaining({id:'saved',texture:1.2})]));
});
it('surfaces failed saves and keeps before/after available',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  vi.mocked(nativeRetouch.edit).mockRejectedValueOnce(new Error('Cannot save recipe'));
  fireEvent.click(screen.getByText('Apply retouch'));
  expect((await screen.findByRole('alert')).textContent).toContain('Cannot save recipe');
  await waitFor(()=>expect((screen.getByText('Show before retouch') as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByText('Show before retouch'));expect(screen.getByAltText('Before native retouch')).toBeTruthy();
});
it('authors a brush mask using keyboard coordinates and saves one operation',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'dodge'}});
  fireEvent.click(screen.getByText('Brush (B)'));
  fireEvent.change(screen.getByLabelText('Center X (%)'),{target:{value:'30'}});
  fireEvent.click(screen.getByText('Dab at target coordinates'));
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[
    expect.objectContaining({mask:{strokes:[expect.objectContaining({erase:false,points:[[.3,.45,1]]})]}})
  ]));
});
it('duplicates and reorders saved operations through native history actions',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue([{...freshRetouch(),id:'one'},{...freshRetouch(),id:'two'}]);
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByLabelText('Move retouch 2 earlier'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','earlier',[],'two'));
  await waitFor(()=>expect((screen.getByLabelText('Duplicate retouch 1') as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByLabelText('Duplicate retouch 1'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','duplicate',[],'one'));
});
it('requires a skin sample and saves range, detail and edge controls with full-frame selection',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'skin_smooth'}});
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText('Pick skin sample on photo')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Source X (%)'),{target:{value:'45'}});
  fireEvent.change(screen.getByLabelText('Source Y (%)'),{target:{value:'40'}});
  fireEvent.change(screen.getByLabelText('Skin color tolerance (8%)'),{target:{value:'0.12'}});
  fireEvent.change(screen.getByLabelText('Edge protection (80%)'),{target:{value:'0.9'}});
  fireEvent.change(screen.getByLabelText(/Fine detail \(100%\)/),{target:{value:'1.1'}});
  fireEvent.click(screen.getByText('Use full photo selection'));
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[
    expect.objectContaining({tool:'skin_smooth',source:[.45,.4],region:[.5,.5,1,1],mask:null,texture:1.1,skin:{tolerance:.12,edgeProtection:.9}})
  ]));
});
