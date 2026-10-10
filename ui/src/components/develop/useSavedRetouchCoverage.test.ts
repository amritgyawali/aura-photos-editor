import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { nativeRetouch, type SelectionPreview } from '../../ipc/nativeRetouch';
import { useSavedRetouchCoverage } from './useSavedRetouchCoverage';
vi.mock('../../ipc/nativeRetouch',()=>({nativeRetouch:{savedSelection:vi.fn()}}));
vi.mock('../../ipc/client',()=>({asIpcError:(error:Error)=>({message:error.message})}));
beforeEach(()=>vi.resetAllMocks());
it('discards late results after changing photo or recipe and clears old coverage during loading',async()=>{
  const resolves: ((image:SelectionPreview)=>void)[]=[];
  vi.mocked(nativeRetouch.savedSelection).mockImplementation(()=>new Promise(resolve=>resolves.push(resolve)));
  const {result,rerender}=renderHook(({photo,revision})=>useSavedRetouchCoverage('p',photo,revision,null,true),{initialProps:{photo:'a',revision:'1'}});
  rerender({photo:'b',revision:'2'});
  const image={width:1,height:1,rgbBase64:'AAAA'};
  await act(async()=>resolves[0]?.(image));
  expect(result.current.image).toBeNull();
  expect(result.current.pending).toBe(true);
  await act(async()=>resolves[1]?.(image));
  await waitFor(()=>expect(result.current.image).toEqual(image));
  rerender({photo:'b',revision:'3'});
  expect(result.current.image).toBeNull();
});
it('surfaces backend errors without painting an empty successful mask',async()=>{
  vi.mocked(nativeRetouch.savedSelection).mockRejectedValue(new Error('Stored skin mask is missing'));
  const {result}=renderHook(()=>useSavedRetouchCoverage('p','a','1',null,true));
  await waitFor(()=>expect(result.current.error).toBe('Stored skin mask is missing'));
  expect(result.current.image).toBeNull();
  expect(result.current.pending).toBe(false);
});
