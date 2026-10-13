import { afterEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { BlemishReview } from './BlemishReview';
import { freshRetouch, nativeRetouch } from '../../ipc/nativeRetouch';
vi.mock('../../ipc/nativeRetouch', async importOriginal=>({...(await importOriginal<typeof import('../../ipc/nativeRetouch')>()),nativeRetouch:{review:vi.fn()}}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
const result={recipeHash:'saved',width:640,height:960,review:{regions:[{region:[.5,.5,.02,.03] as [number,number,number,number],pixels:24}],measuredAreas:1,selectedMarkPixels:24}};
const props=()=>({projectId:'project',photoId:'photo',recipeHash:'saved',selection:freshRetouch(),disabled:false,canPrepare:true,onPrepare:vi.fn(),onChecking:vi.fn()});
describe('remaining mark review',()=>{
  it('prepares the measured region only after an explicit user choice',async()=>{
    vi.mocked(nativeRetouch.review).mockResolvedValue(result);
    const p=props();render(<BlemishReview {...p}/>);
    fireEvent.click(screen.getByRole('button',{name:'Check remaining marks'}));
    const use=await screen.findByRole('button',{name:'Prepare blemish brush here'});
    expect(p.onPrepare).not.toHaveBeenCalled();fireEvent.click(use);
    expect(p.onPrepare).toHaveBeenCalledWith(result.review.regions[0]!.region);
    expect(p.onChecking.mock.calls.map(v=>v[0])).toEqual([true,false]);
  });
  it('does not claim complete removal when no candidates are measured',async()=>{
    vi.mocked(nativeRetouch.review).mockResolvedValue({...result,review:{regions:[],measuredAreas:0,selectedMarkPixels:0}});
    render(<BlemishReview {...props()}/>);fireEvent.click(screen.getByRole('button',{name:'Check remaining marks'}));
    expect((await screen.findByRole('status')).textContent).toContain('does not verify complete removal');
  });
  it('refuses a late result belonging to a different photo',async()=>{
    let resolve!:(value:typeof result)=>void;
    vi.mocked(nativeRetouch.review).mockReturnValue(new Promise(done=>{resolve=done;}));
    const p=props();const view=render(<BlemishReview {...p}/>);fireEvent.click(screen.getByRole('button',{name:'Check remaining marks'}));
    view.rerender(<BlemishReview {...p} photoId="other"/>);
    await act(async()=>{resolve(result);});expect(screen.queryByRole('status')).toBeNull();
  });
  it('keeps native errors visible and never creates a success result',async()=>{
    vi.mocked(nativeRetouch.review).mockRejectedValue({code:'AURA-DB-3006',message:'The saved photo changed.',runbookUrl:'https://aura.app/e/AURA-DB-3006',retryable:false});
    render(<BlemishReview {...props()}/>);fireEvent.click(screen.getByRole('button',{name:'Check remaining marks'}));
    expect((await screen.findByRole('alert')).textContent).toContain('The saved photo changed');
    expect(screen.queryByRole('status')).toBeNull();
  });
});
