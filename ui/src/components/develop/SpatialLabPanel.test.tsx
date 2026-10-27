import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {expect,it,vi}from'vitest';
import {SpatialLabPanel}from'./SpatialLabPanel';
import {SPATIAL_TOOLS,SPATIAL_ADDITION_CONTROLS,spatialLab}from'../../ipc/spatialLab';
import type{RecipeDto}from'../../ipc/types';
vi.mock('../../ipc/spatialLab',async original=>({...await original<typeof import('../../ipc/spatialLab')>(),spatialLab:{save:vi.fn()}}));
const recipe={photoId:'photo',recipeHash:'current',body:JSON.stringify({studio_masks_v1:[{id:'selection',name:'Selection',enabled:true,amount:1,params:{},components:[]}]})}as RecipeDto;
it.each([0,25,50,75])('authors actual tools in bounded revision-bound stacks, batch %s',async(start)=>{
    const onApply=vi.fn();vi.mocked(spatialLab.save).mockResolvedValue({recipe});render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
    const tools=SPATIAL_TOOLS.slice(start,start+25);
    for(const[id]of tools){fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:id}});fireEvent.click(screen.getByText('Add spatial operation'));}
    fireEvent.click(screen.getByText('Apply spatial tools'));await waitFor(()=>expect(onApply).toHaveBeenCalledWith(recipe));const call=vi.mocked(spatialLab.save).mock.calls.at(-1)!;expect(call.slice(0,3)).toEqual(['project','photo','current']);expect(call[3].map(v=>v.tool)).toEqual(tools.map(v=>v[0]));
});

it.each(SPATIAL_TOOLS.slice(50))('exposes the effective controls and description for %s',async(id,_label,description)=>{
    vi.mocked(spatialLab.save).mockResolvedValue({recipe});
    render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
    fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:id}});
    fireEvent.click(screen.getByText('Add spatial operation'));
    expect(screen.getAllByText(description).length).toBeGreaterThan(0);
    expect(screen.queryByLabelText('Spatial contrast threshold')).toBeNull();
    const controls=SPATIAL_ADDITION_CONTROLS[id]!;
    for(const item of controls){
        const value=item.field==='angle'?22:item.field==='detail'?0.8:item.field.startsWith('source')?0.65:item.field.startsWith('shift')?0.03:item.field==='radius'?0.08:0.1;
        fireEvent.change(screen.getByLabelText(item.label),{target:{value:String(value)}});
        expect((screen.getByLabelText(item.label)as HTMLInputElement).value).toBe(String(value));
    }
    expect(screen.getAllByRole('spinbutton')).toHaveLength(1+controls.length);
    fireEvent.click(screen.getByText('Apply spatial tools'));
    await waitFor(()=>expect(spatialLab.save).toHaveBeenLastCalledWith('project','photo','current',[expect.objectContaining({tool:id})]));
});
it('edits motion path and retains drafts across equivalent revisions',()=>{
    const onApply=vi.fn();const view=render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);
    fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:'trajectory_blur'}});fireEvent.click(screen.getByText('Add spatial operation'));fireEvent.change(screen.getByLabelText('Path 1 horizontal'),{target:{value:'0.08'}});view.rerender(<SpatialLabPanel projectId="project" photoId="photo" recipe={{...recipe}} disabled={false} onApply={onApply}/>);expect((screen.getByLabelText('Path 1 horizontal')as HTMLInputElement).value).toBe('0.08');
    fireEvent.click(screen.getByText('Discard spatial draft'));expect(screen.queryByLabelText('Selected spatial operation')).toBeNull();
});
it('prevents missing selections and ignores late saves after navigation',async()=>{
    let resolve!:(value:{recipe:RecipeDto})=>void;vi.mocked(spatialLab.save).mockReturnValue(new Promise(done=>{resolve=done;}));const onApply=vi.fn();const view=render(<SpatialLabPanel projectId="project" photoId="photo" recipe={{...recipe,body:'{}'}} disabled={false} onApply={onApply}/>);expect((screen.getByText('Add spatial operation')as HTMLButtonElement).disabled).toBe(true);
    view.rerender(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={onApply}/>);fireEvent.click(screen.getByText('Add spatial operation'));fireEvent.click(screen.getByText('Apply spatial tools'));view.rerender(<SpatialLabPanel projectId="project" photoId="next" recipe={{...recipe,photoId:'next',recipeHash:'next'}} disabled={false} onApply={onApply}/>);resolve({recipe});await waitFor(()=>expect(onApply).not.toHaveBeenCalled());
});

it('reorders spatial operations and hides controls unused by each method',async()=>{
    vi.mocked(spatialLab.save).mockResolvedValue({recipe});
    render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
    for(const tool of ['vector_medoid','trimmed_mean']){
        fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:tool}});
        fireEvent.click(screen.getByText('Add spatial operation'));
        expect(screen.queryByLabelText('Spatial contrast threshold')).toBeNull();
    }
    fireEvent.click(screen.getByText('Move spatial operation earlier'));
    expect((screen.getByText('Move spatial operation earlier') as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText('Apply spatial tools'));
    await waitFor(()=>expect(spatialLab.save).toHaveBeenLastCalledWith('project','photo','current',expect.arrayContaining([expect.objectContaining({tool:'trimmed_mean'})])));
    expect(vi.mocked(spatialLab.save).mock.calls.at(-1)![3].map(op=>op.tool)).toEqual(['trimmed_mean','vector_medoid']);
});

it('keeps a rejected spatial draft and dismisses the failure on discard',async()=>{
    vi.mocked(spatialLab.save).mockRejectedValue({message:'Saved recipe changed',code:'AURA-RENDER-8002'});
    render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
    fireEvent.click(screen.getByText('Add spatial operation'));
    fireEvent.change(screen.getByLabelText('Spatial strength'),{target:{value:'0.8'}});
    fireEvent.click(screen.getByText('Apply spatial tools'));
    await screen.findByRole('alert');
    expect((screen.getByLabelText('Spatial strength') as HTMLInputElement).value).toBe('0.8');
    fireEvent.click(screen.getByText('Discard spatial draft'));
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByLabelText('Selected spatial operation')).toBeNull();
});

it('exposes only the effective optical and estimator controls',()=>{
    render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
    for(const tool of ['geometric_mean','radial_chromatic','tilt_shift_integration']){
        fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:tool}});fireEvent.click(screen.getByText('Add spatial operation'));
        if(tool==='geometric_mean'){
            expect(screen.queryByLabelText('Source horizontal')).toBeNull();
            expect(screen.queryByLabelText('Spatial contrast threshold')).toBeNull();
        }else{
            expect(screen.getByLabelText('Source horizontal')).toBeTruthy();
            if(tool==='radial_chromatic')expect(screen.getByLabelText('Red radial coefficient')).toBeTruthy();
            else expect(screen.getByLabelText('Spatial angle')).toBeTruthy();
        }
    }
});

it.each([
    ['radial_chromatic','Red radial coefficient','Blue radial coefficient'],
    ['perspective_plane','Horizontal perspective coefficient','Vertical perspective coefficient'],
    ['thin_plate_warp','Horizontal warp displacement','Vertical warp displacement'],
])('labels coordinate parameters according to the %s algorithm',async(tool,first,second)=>{
    vi.mocked(spatialLab.save).mockResolvedValue({recipe});
    render(<SpatialLabPanel projectId="project" photoId="photo" recipe={recipe} disabled={false} onApply={vi.fn()}/>);
    fireEvent.change(screen.getByLabelText('Spatial tool'),{target:{value:tool}});
    fireEvent.click(screen.getByText('Add spatial operation'));
    fireEvent.change(screen.getByLabelText(first!),{target:{value:'0.04'}});
    fireEvent.change(screen.getByLabelText(second!),{target:{value:'-0.03'}});
    expect(screen.queryByLabelText('Channel horizontal shift')).toBeNull();
    fireEvent.click(screen.getByText('Apply spatial tools'));
    await waitFor(()=>expect(spatialLab.save).toHaveBeenLastCalledWith('project','photo','current',[expect.objectContaining({tool,shift:[0.04,-0.03]})]));
});
