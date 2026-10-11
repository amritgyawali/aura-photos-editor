import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { InternalAiStatus } from './InternalAiStatus';
import { api } from '../ipc/client';
vi.mock('../ipc/client', () => ({ inTauri:()=>true, asIpcError:(e:unknown)=>e, api:{internalAiStatus:vi.fn()} }));
afterEach(()=>{cleanup();vi.clearAllMocks();});
describe('internal AI status',()=>{
  it('loads automatically and reports image capability and unavailable pricing honestly',async()=>{
    vi.mocked(api.internalAiStatus).mockResolvedValue({sourceAvailable:true,checkedDay:'2026-10-11',profiles:[
      {id:'VISION',endpoint:'https://vision.example/v1',model:'vision-model',vision:true,inputUsdPerMtok:1,outputUsdPerMtok:2,status:'catalogue_ready'},
      {id:'TEXT',endpoint:'https://text.example/v1',model:'text-model',vision:false,inputUsdPerMtok:null,outputUsdPerMtok:null,status:'http_401'}]});
    render(<InternalAiStatus/>);
    expect((await screen.findByText(/1 photo routes available/)).textContent).toContain('do not prove');
    expect(screen.getByText(/TEXT: text-model/).textContent).toContain('pricing needed');
    expect(api.internalAiStatus).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole('button',{name:'Refresh internal providers'}));
    expect(api.internalAiStatus).toHaveBeenLastCalledWith(true);
  });
  it('keeps connection errors visible',async()=>{
    vi.mocked(api.internalAiStatus).mockRejectedValue({message:'Configuration could not be loaded'});
    render(<InternalAiStatus/>);
    expect((await screen.findByRole('alert')).textContent).toContain('could not be loaded');
    expect(screen.queryByText(/photo routes available/)).toBeNull();
  });
});
