import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { NativeRetouchWorkspace } from './NativeRetouchWorkspace';
import { nativeRetouch, freshRetouch } from '../../ipc/nativeRetouch';
import { develop } from '../../ipc/client';

vi.mock('../../ipc/nativeRetouch',async()=>({...await vi.importActual('../../ipc/nativeRetouch'),nativeRetouch:{autoPortrait:vi.fn(),autoRetouch:vi.fn(),edit:vi.fn(),preview:vi.fn(),draftPreview:vi.fn(),selectionPreview:vi.fn(),savedSelection:vi.fn()}}));
vi.mock('../../ipc/client',()=>({asIpcError:(e:Error)=>({message:e.message}),develop:{imageRecipe:vi.fn(),imageHistory:vi.fn(),historyStep:vi.fn()}}));
beforeEach(()=>{
  vi.resetAllMocks();vi.mocked(nativeRetouch.edit).mockResolvedValue([]);
  vi.mocked(nativeRetouch.preview).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(120,100,90)),notes:[]} as never);
  vi.mocked(nativeRetouch.selectionPreview).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(255,255,255))});
  vi.mocked(nativeRetouch.savedSelection).mockResolvedValue({width:1,height:1,rgbBase64:btoa(String.fromCharCode(255,255,255))});
  vi.mocked(develop.imageHistory).mockResolvedValue({canUndo:true,canRedo:false} as never);
  localStorage.clear();
});
function open(){return render(<NativeRetouchWorkspace projectId="project" photoId="photo" onClose={vi.fn()} onBusyChange={vi.fn()}/>);}
it('keeps manual repair available after more than 256 saved acne repairs',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue(Array.from({length:300},(_,i)=>({...freshRetouch(),id:`saved-${i}`})));
  open();
  await screen.findByAltText('Retouched photograph');
  const duplicate=screen.getByRole('button',{name:'Duplicate retouch 300'});
  expect((duplicate as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(duplicate);
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','duplicate',[],'saved-299'));
});
it('uses grayscale contrast review for both sides without changing the recipe or filtering coverage',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue([{...freshRetouch(),id:'saved'}]);
  open();
  const image=await screen.findByAltText('Retouched photograph') as HTMLImageElement;
  const source=image.src;
  fireEvent.change(screen.getByLabelText('Skin analysis view'),{target:{value:'high-contrast'}});
  expect(image.style.filter).toBe('grayscale(1) contrast(4)');
  fireEvent.click(screen.getByRole('button',{name:'Split comparison'}));
  expect((screen.getByAltText('Before native retouch comparison') as HTMLImageElement).style.filter).toBe(image.style.filter);
  fireEvent.change(screen.getByLabelText('Skin analysis view'),{target:{value:'low-contrast'}});
  expect(image.style.filter).toBe('grayscale(1) contrast(0.25)');
  fireEvent.click(screen.getByRole('button',{name:'Show retouched areas'}));
  const coverage=await screen.findByAltText('Saved retouch coverage') as HTMLImageElement;
  expect(coverage.style.filter).toBe('');
  fireEvent.click(screen.getByRole('button',{name:'Show retouched areas'}));
  fireEvent.change(screen.getByLabelText('Skin analysis view'),{target:{value:'color'}});
  expect((screen.getByAltText('Retouched photograph') as HTMLImageElement).src).toBe(source);
  expect(nativeRetouch.preview).toHaveBeenCalledTimes(2);
  expect(nativeRetouch.draftPreview).not.toHaveBeenCalled();
  expect(vi.mocked(nativeRetouch.edit).mock.calls.every(call=>call[2]==='list')).toBe(true);
});
it('clears stale saved coverage after a failed refresh and allows retry without writing edits',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue([{...freshRetouch(),id:'saved'}]);
  const props={projectId:'project',photoId:'photo',onClose:vi.fn(),onBusyChange:vi.fn()};
  const view=render(<NativeRetouchWorkspace {...props} revision={0}/>);
  await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByRole('button',{name:'Show retouched areas'}));
  await screen.findByAltText('Saved retouch coverage');
  vi.mocked(nativeRetouch.preview).mockRejectedValueOnce(new Error('Preview unavailable'));
  view.rerender(<NativeRetouchWorkspace {...props} revision={1}/>);
  await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('Preview unavailable'));
  expect(screen.queryByAltText('Saved retouch coverage')).toBeNull();
  expect(screen.queryByAltText('Retouched photograph')).toBeNull();
  expect((screen.getByText('Apply retouch') as HTMLButtonElement).matches(':disabled')).toBe(true);
  expect((screen.getByText('Undo') as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByRole('button',{name:'Back to Develop'}) as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(screen.getByRole('button',{name:'Reload retouch'}));
  await screen.findByAltText('Saved retouch coverage');
  expect(vi.mocked(nativeRetouch.edit).mock.calls.every(call=>call[2]==='list')).toBe(true);
});
it('shows saved coverage for the whole stack or one step without creating a draft or writing edits',async()=>{
  vi.mocked(nativeRetouch.edit).mockResolvedValue([{...freshRetouch(),id:'saved'}]);
  open(); await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByRole('button',{name:'Show retouched areas'}));
  await screen.findByAltText('Saved retouch coverage');
  expect(nativeRetouch.savedSelection).toHaveBeenCalledWith('project','photo',null);
  fireEvent.change(screen.getByLabelText('Show selection for'),{target:{value:'saved'}});
  await waitFor(()=>expect(nativeRetouch.savedSelection).toHaveBeenLastCalledWith('project','photo','saved'));
  await screen.findByAltText('Saved retouch coverage');
  fireEvent.change(screen.getByLabelText('Overlay visibility'),{target:{value:'.8'}});
  expect(nativeRetouch.savedSelection).toHaveBeenCalledTimes(2);
  expect(nativeRetouch.draftPreview).not.toHaveBeenCalled();
  fireEvent.keyDown(screen.getByRole('region',{name:'Native retouch workspace'}),{key:'Enter'});
  expect(vi.mocked(nativeRetouch.edit).mock.calls.every(call=>call[2]==='list')).toBe(true);
  fireEvent.click(screen.getByRole('button',{name:'Show before retouch'}));
  expect(screen.queryByAltText('Saved retouch coverage')).toBeNull();
});
it('restores distinct saved retouch choices after switching photos and undoing', async () => {
  const saved = (photoId: string, recipeHash: string, scope: string, smoothing: number) => ({ photoId, recipeHash,
    body: JSON.stringify({ studio_portrait_auto_v1: { options: { scope, intensity: .6, teeth: false, settings: { smoothing } } } }) }) as never;
  vi.mocked(develop.imageRecipe).mockResolvedValueOnce(saved('a', 'a1', 'body', .2));
  const props = { projectId: 'project', onClose: vi.fn(), onBusyChange: vi.fn() };
  const { rerender } = render(<NativeRetouchWorkspace {...props} photoId="a" />);
  await waitFor(() => expect((screen.getByRole('button', { name: 'Auto retouch: Body skin' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByText('Skin'));
  expect((screen.getByLabelText('Skin smoothing') as HTMLInputElement).value).toBe('20');
  vi.mocked(develop.imageRecipe).mockResolvedValueOnce(saved('b', 'b1', 'face', .8));
  rerender(<NativeRetouchWorkspace {...props} photoId="b" />);
  await waitFor(() => expect((screen.getByLabelText('Skin smoothing') as HTMLInputElement).value).toBe('80'));
  fireEvent.click(screen.getByRole('button', { name: 'Auto retouch: Face' }));
  await waitFor(() => expect(nativeRetouch.autoRetouch).toHaveBeenCalledWith('project', 'b', expect.objectContaining({
    scope: 'face', intensity: .6, teeth: false, settings: expect.objectContaining({ smoothing: .8 }),
  })));
  await waitFor(() => expect((screen.getByText('Undo') as HTMLButtonElement).disabled).toBe(false));
  vi.mocked(develop.imageRecipe).mockResolvedValueOnce(saved('b', 'b0', 'face_and_body', .35));
  fireEvent.click(screen.getByText('Undo'));
  await waitFor(() => expect((screen.getByLabelText('Skin smoothing') as HTMLInputElement).value).toBe('35'));
  expect(screen.getByRole('button', { name: 'Auto retouch: Face + body skin' })).toBeTruthy();
  vi.mocked(develop.imageRecipe).mockRejectedValueOnce(new Error('Cannot load photo c'));
  rerender(<NativeRetouchWorkspace {...props} photoId="c" />);
  await screen.findByText('Cannot load photo c');
  const automatic = screen.getByRole('button', { name: 'Auto retouch: Face' });
  expect(automatic.closest('fieldset')?.disabled).toBe(true);
  fireEvent.click(automatic);
  expect(nativeRetouch.autoRetouch).toHaveBeenCalledTimes(1);
});
it('saves separate fine skin texture control with frequency separation', async () => {
  open(); await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'), { target: { value: 'frequency' } });
  fireEvent.click(screen.getByLabelText('Preserve fine skin texture'));
  fireEvent.change(screen.getByLabelText('Texture gain (100%)'), { target: { value: '.85' } });
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'append', [
    expect.objectContaining({ tool: 'frequency', preserveMicrotexture: true, texture: .85 }),
  ]));
});
it('saves frequency healing with its own sensitivity and dark-mark protection', async () => {
  open(); await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'), { target: { value: 'frequency_heal' } });
  expect(screen.getByText(/skin with nothing wrong with it is left exactly as it is/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Mark sensitivity (50%)'), { target: { value: '.8' } });
  fireEvent.click(screen.getByLabelText('Keep dark marks (moles, freckles)'));
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'append', [
    expect.objectContaining({ tool: 'frequency_heal', sensitivity: .8, keepDarkMarks: true }),
  ]));
});
it('saves a texture graft with its level and glint limit, with no source required', async () => {
  open(); await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'), { target: { value: 'texture_graft' } });
  expect(screen.getByText(/nothing is generated/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText('Texture level (100%)'), { target: { value: '.9' } });
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'append', [
    expect.objectContaining({ tool: 'texture_graft', texture: .9, source: null }),
  ]));
});
it('edits a smaller patch donor and resets its scale when the source is cleared', async () => {
  open(); await screen.findByAltText('Retouched photograph');
  fireEvent.change(screen.getByLabelText('Tool'), { target: { value: 'patch_heal' } });
  fireEvent.change(screen.getByLabelText('Source X (%)'), { target: { value: '40' } });
  fireEvent.change(screen.getByLabelText('Source patch size'), { target: { value: '.35' } });
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'append', [
    expect.objectContaining({ tool: 'patch_heal', sourceScale: .35, source: [.4, .5] }),
  ]));
  await screen.findByAltText('Retouched photograph');
  await waitFor(() => expect((screen.getByText('Clear source') as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByText('Clear source'));
  fireEvent.click(screen.getByText('Apply retouch'));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'append', [
    expect.objectContaining({ source: null, sourceScale: 1 }),
  ]));
});
it('clears the AI restriction when selecting the entire photo', async () => {
  const saved = { ...freshRetouch(), id: 'auto-portrait-v1-0-texture', tool: 'frequency' as const, matte: 'auto-portrait-v1-0-face' };
  vi.mocked(nativeRetouch.edit).mockResolvedValue([saved]);
  open();
  await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByRole('button', { name: /1\. Frequency separation/ }));
  expect(screen.getByText(/AI mask active/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Select entire photo' }));
  fireEvent.click(screen.getByRole('button', { name: 'Update selected retouch' }));
  await waitFor(() => expect(nativeRetouch.edit).toHaveBeenCalledWith('project', 'photo', 'update', [
    expect.objectContaining({ matte: null, mask: null, region: [.5, .5, 1, 1] }),
  ]));
});
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
it('keeps the versioned clean-ring fit when manually adjusting an automatic heal',async()=>{
  const saved={...freshRetouch(),id:'saved-heal',tool:'patch_heal' as const,textureHeal:true,cleanRingFit:true,curvedHeal:true,healSamples:[[1.5,0],[0,1.5]] as [number,number][],textureSources:[[.2,.2],[.3,.2],[.4,.2]] as [number,number][],sourceScale:.5,source:[.3,.4] as [number,number]};
  vi.mocked(nativeRetouch.edit).mockResolvedValue([saved]);
  open();await screen.findByAltText('Retouched photograph');
  fireEvent.click(screen.getByText(/1\. Texture-aware patch heal/));
  fireEvent.change(screen.getByLabelText('Strength (65%)'),{target:{value:'.9'}});
  fireEvent.click(screen.getByText('Update selected retouch'));
  await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','update',[
    expect.objectContaining({id:'saved-heal',amount:.9,textureHeal:true,cleanRingFit:true,curvedHeal:true,healSamples:[[1.5,0],[0,1.5]],textureSources:[[.2,.2],[.3,.2],[.4,.2]],sourceScale:.5,source:[.3,.4]}),
  ]));
});
  it('clears additional donors when manually changing an automatic patch size',async()=>{
    const saved={...freshRetouch(),id:'saved-heal',tool:'patch_heal' as const,textureHeal:true,curvedHeal:true,healSamples:[[1.5,0],[0,1.5]] as [number,number][],textureSources:[[.2,.2],[.3,.2],[.4,.2]] as [number,number][],sourceScale:.5,source:[.3,.4] as [number,number]};
    vi.mocked(nativeRetouch.edit).mockResolvedValue([saved]);open();await screen.findByAltText('Retouched photograph');
    fireEvent.click(screen.getByText(/1\. Texture-aware patch heal/));
    fireEvent.change(screen.getByLabelText('Source patch size'),{target:{value:'1'}});
    fireEvent.click(screen.getByText('Update selected retouch'));
    await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','update',[
      expect.objectContaining({sourceScale:1,textureSources:[],healSamples:saved.healSamples}),
    ]));
  });
  it('clears measured lighting and donor footprints when manually moving an automatic heal',async()=>{
    const saved={...freshRetouch(),id:'saved-heal',tool:'patch_heal' as const,textureHeal:true,curvedHeal:true,healSamples:[[1.5,0],[0,1.5]] as [number,number][],textureSources:[[.2,.2],[.3,.2],[.4,.2]] as [number,number][],sourceScale:.5,source:[.3,.4] as [number,number]};
    vi.mocked(nativeRetouch.edit).mockResolvedValue([saved]);open();await screen.findByAltText('Retouched photograph');
    fireEvent.click(screen.getByText(/1\. Texture-aware patch heal/));
    fireEvent.change(screen.getByLabelText('Center X (%)'),{target:{value:'60'}});
    fireEvent.click(screen.getByText('Update selected retouch'));
    await waitFor(()=>expect(nativeRetouch.edit).toHaveBeenCalledWith('project','photo','update',[
      expect.objectContaining({region:[.6,saved.region[1],saved.region[2],saved.region[3]],healSamples:[],textureSources:[]}),
    ]));
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
    expect.objectContaining({tool:'skin_smooth',source:[.45,.4],region:[.5,.5,1,1],mask:null,texture:1.1,skin:{tolerance:.12,edgeProtection:.9,connected:true}})
  ]));
});
