import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { NativeRetouchWorkspace } from './NativeRetouchWorkspace';
import { nativeRetouch, freshRetouch } from '../../ipc/nativeRetouch';
import { develop } from '../../ipc/client';

vi.mock('../../ipc/nativeRetouch',async()=>({...await vi.importActual('../../ipc/nativeRetouch'),nativeRetouch:{autoPortrait:vi.fn(),edit:vi.fn(),preview:vi.fn(),draftPreview:vi.fn(),selectionPreview:vi.fn()}}));
vi.mock('../../ipc/client',()=>({asIpcError:(e:Error)=>({message:e.message}),develop:{imageRecipe:vi.fn(),imageHistory:vi.fn(),historyStep:vi.fn()}}));
beforeEach(()=>{
  vi.resetAllMocks();vi.mocked(nativeRetouch.edit).mockResolvedValue([]);
  vi.mocked(nativeRetouch.preview).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(120,100,90)),notes:[]} as never);
  vi.mocked(nativeRetouch.selectionPreview).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(255,255,255))});
  vi.mocked(develop.imageHistory).mockResolvedValue({canUndo:true,canRedo:false} as never);
  localStorage.clear();
});
function open(){return render(<NativeRetouchWorkspace projectId="project" photoId="photo" onClose={vi.fn()} onBusyChange={vi.fn()}/>);}
it('automatically retouches once, refreshes saved steps and report, and supports undo',async()=>{
  let finish: ()=>void = ()=>{};
  vi.mocked(nativeRetouch.autoPortrait).mockImplementation(()=>new Promise(resolve=>{finish=()=>resolve({} as never);}));
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByText('Auto portrait'));
  expect((screen.getByText('Detecting faces and preparing skin retouch…') as HTMLButtonElement).disabled).toBe(true);
  expect(nativeRetouch.autoPortrait).toHaveBeenCalledTimes(1);
  vi.mocked(develop.imageRecipe).mockResolvedValue({body:JSON.stringify({studio_portrait_auto_v1:{message:'Retouched 1 of 1 detected faces.'}})} as never);
  finish();await screen.findByText('Last automatic pass: Retouched 1 of 1 detected faces.');
  fireEvent.click(screen.getByText('Undo'));
  await waitFor(()=>expect(develop.historyStep).toHaveBeenCalledWith({projectId:'project',photoId:'photo',action:'undo'}));
});
it('protects a draft from automatic editing and surfaces detection failures',async()=>{
  vi.mocked(nativeRetouch.autoPortrait).mockRejectedValue(new Error('Portrait analysis failed'));
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'dodge'}});
  expect((screen.getByText('Auto portrait') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByText('Discard draft'));
  fireEvent.click(screen.getByText('Auto portrait'));
  await screen.findByText('Portrait analysis failed');
  expect((screen.getByText('Auto portrait') as HTMLButtonElement).disabled).toBe(false);
});
it('previews and saves gradient inversion with a brightness range without saving during preview',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'dodge'}});
  fireEvent.click(screen.getByText('Gradient (G)'));
  fireEvent.click(screen.getByLabelText('Outside shape'));
  fireEvent.click(screen.getByLabelText('Limit by brightness'));
  fireEvent.click(screen.getByText('Highlights'));
  const calls=vi.mocked(nativeRetouch.edit).mock.calls.length;
  fireEvent.click(screen.getByText('Preview selection mask'));
  await screen.findByAltText('Selection mask');
  expect(nativeRetouch.edit).toHaveBeenCalledTimes(calls);
  expect(nativeRetouch.selectionPreview).toHaveBeenCalledWith('project','photo',expect.objectContaining({
    selection:{inverted:true,gradient:{start:[.2,.5],end:[.8,.5]},luminance:{low:1,high:16,softness:1}}
  }),null);
  fireEvent.click(screen.getByText('Preview selection mask'));
  await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','append',[
    expect.objectContaining({selection:expect.objectContaining({inverted:true,luminance:{low:1,high:16,softness:1}})})
  ]));
});
it('rejects crossed brightness limits and clears shape constraints when selecting the entire photo',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'dodge'}});
  fireEvent.click(screen.getByText('Gradient (G)'));
  fireEvent.click(screen.getByLabelText('Outside shape'));
  fireEvent.click(screen.getByLabelText('Limit by brightness'));
  fireEvent.change(screen.getByLabelText('Dark limit (EV)'),{target:{value:'3'}});
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByText('Midtones'));
  fireEvent.click(screen.getByText('Select entire photo'));
  expect(screen.queryByLabelText('Gradient start X (%)')).toBeNull();
  expect((screen.getByLabelText('Outside shape') as HTMLInputElement).checked).toBe(false);
  expect((screen.getByLabelText('Limit by brightness') as HTMLInputElement).checked).toBe(true);
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).disabled).toBe(false);
});
it('compares cached previews without editing the recipe or requesting another render',async()=>{
  open();await screen.findByAltText('Retouched photograph');
  const renders=vi.mocked(nativeRetouch.preview).mock.calls.length;
  const edits=vi.mocked(nativeRetouch.edit).mock.calls.length;
  fireEvent.click(screen.getByText('Split comparison'));
  expect(screen.getByAltText('Before native retouch comparison')).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Before/after split'),{target:{value:'80'}});
  fireEvent.click(screen.getByText('Show before retouch'));
  expect(screen.queryByLabelText('Before/after split')).toBeNull();
  fireEvent.click(screen.getByText('Split comparison'));
  expect(screen.getByLabelText('Before/after split')).toBeTruthy();
  expect(nativeRetouch.preview).toHaveBeenCalledTimes(renders);
  expect(nativeRetouch.edit).toHaveBeenCalledTimes(edits);
});
it('preserves dirty drafts across saved-operation controls and history shortcuts',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue([{...freshRetouch(),id:'saved',tool:'frequency'}]);
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Center X (%)'),{target:{value:'61'}});
  for(const name of ['Undo','Clear native retouch','Duplicate retouch 1','Remove retouch 1']) {
    expect((screen.getByRole('button',{name}) as HTMLButtonElement).disabled).toBe(true);
  }
  expect((screen.getByRole('button',{name:/^1\. Frequency separation/}) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.keyDown(screen.getByLabelText('Retouch image interaction'),{key:'z',ctrlKey:true});
  expect(develop.historyStep).not.toHaveBeenCalled();
  expect((screen.getByLabelText('Center X (%)') as HTMLInputElement).value).toBe('61');
  fireEvent.click(screen.getByText('Discard draft'));
  await waitFor(()=>expect((screen.getByText('Undo') as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button',{name:/^1\. Frequency separation/}));
  fireEvent.change(screen.getByLabelText('Texture gain (100%)'),{target:{value:'1.2'}});
  fireEvent.change(screen.getByLabelText('Tool'),{target:{value:'burn'}});
  expect((screen.getByLabelText('Tool') as HTMLSelectElement).value).toBe('frequency');
  expect((screen.getByText('Start another operation') as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByText('Natural skin in selection') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByText('Update selected retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','update',[
    expect.objectContaining({id:'saved',tool:'frequency',texture:1.2})
  ]));
});
it('does not compare previews with different pixel dimensions',async()=>{
  vi.mocked(nativeRetouch.preview).mockImplementation(async(_project,_photo,before)=>({
    width:before?2:1,height:1,rgbBase64:btoa(String.fromCharCode(...Array(before?6:3).fill(100))),notes:[],
  } as never));
  open();await screen.findByAltText('Retouched photograph');
  expect((screen.getByText('Split comparison') as HTMLButtonElement).disabled).toBe(true);
});
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
