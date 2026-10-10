import { beforeEach, expect, it, vi } from 'vitest';
import { api } from '../../ipc/client';
import { nativeRetouch, DEFAULT_AUTO_RETOUCH } from '../../ipc/nativeRetouch';
import { retouchCollection } from './batchRetouch';
import type { RecipeDto } from '../../ipc/types';

vi.mock('../../ipc/client',()=>({api:{listImages:vi.fn()},asIpcError:(e:Error)=>({message:e.message})}));
vi.mock('../../ipc/nativeRetouch',async()=>({...await vi.importActual('../../ipc/nativeRetouch'),nativeRetouch:{autoRetouch:vi.fn()}}));
const recipe=(operations:number)=>({body:JSON.stringify({studio_portrait_auto_v1:{operations,message:operations?'Saved cleanup':'No skin found'}})}) as RecipeDto;
beforeEach(()=>vi.resetAllMocks());

it('measures each photo separately, reports skips/failures and continues after a failed photo',async()=>{
  vi.mocked(api.listImages).mockResolvedValue([{id:'a',fileName:'face.jpg'},{id:'b',fileName:'unreadable.jpg'},
    {id:'c',fileName:'scene.jpg'},{id:'a',fileName:'duplicate.jpg'}] as never);
  vi.mocked(nativeRetouch.autoRetouch).mockResolvedValueOnce(recipe(20)).mockRejectedValueOnce(new Error('Unreadable photo')).mockResolvedValueOnce(recipe(0));
  const progress=vi.fn(),completed=vi.fn();
  const result=await retouchCollection('project',DEFAULT_AUTO_RETOUCH,()=>false,progress,completed);
  expect(result.map(r=>r.outcome)).toEqual(['retouched','failed','skipped']);
  expect(nativeRetouch.autoRetouch).toHaveBeenCalledTimes(3);
  expect(vi.mocked(nativeRetouch.autoRetouch).mock.calls.map(c=>c.slice(0,2))).toEqual([['project','a'],['project','b'],['project','c']]);
  expect(vi.mocked(nativeRetouch.autoRetouch).mock.calls.every(c=>c[2]===DEFAULT_AUTO_RETOUCH&&c.length===3)).toBe(true);
  expect(completed).toHaveBeenCalledTimes(3);
  expect(progress).toHaveBeenLastCalledWith('1 retouched, 1 skipped, 1 failed. 0 remaining.');
});

it('stops after the current saved photo and does not start another operation',async()=>{
  vi.mocked(api.listImages).mockResolvedValue([{id:'a',fileName:'a.jpg'},{id:'b',fileName:'b.jpg'}] as never);
  let stop=false;
  vi.mocked(nativeRetouch.autoRetouch).mockImplementation(async()=>{stop=true;return recipe(1);});
  const progress=vi.fn();
  const result=await retouchCollection('project',DEFAULT_AUTO_RETOUCH,()=>stop,progress,vi.fn());
  expect(result).toHaveLength(1);expect(nativeRetouch.autoRetouch).toHaveBeenCalledTimes(1);
  expect(progress).toHaveBeenLastCalledWith('Stopped after the current photo. 1 retouched, 0 skipped, 0 failed. 1 remaining.');
});
