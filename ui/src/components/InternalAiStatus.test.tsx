import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { InternalAiStatus, routeStatus } from './InternalAiStatus';
import { api } from '../ipc/client';
vi.mock('../ipc/client', () => ({ inTauri:()=>true, asIpcError:(e:unknown)=>e, api:{internalAiStatus:vi.fn()} }));
afterEach(()=>{cleanup();vi.clearAllMocks();});
describe('internal AI status',()=>{
  it('loads automatically and reports image capability and unavailable pricing honestly',async()=>{
    vi.mocked(api.internalAiStatus).mockResolvedValue({sourceAvailable:true,checkedDay:'2026-10-11',profiles:[
      {id:'VISION',endpoint:'https://vision.example/v1',model:'vision-model',vision:true,inputUsdPerMtok:1,outputUsdPerMtok:2,status:'catalogue_ready'},
      {id:'TEXT',endpoint:'https://text.example/v1',model:'text-model',vision:false,inputUsdPerMtok:null,outputUsdPerMtok:null,status:'http_401'}]});
    render(<InternalAiStatus/>);
    expect((await screen.findByText(/1 photo routes available/)).textContent).toContain('read a generated test picture');
    expect(screen.getByText(/TEXT: text-model/).textContent).toContain('pricing needed');
    expect(screen.getByText(/TEXT: text-model/).textContent).toContain('did not accept this key');
    expect(screen.getByText(/VISION: vision-model/).textContent).toContain('ready');
    expect(api.internalAiStatus).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByRole('button',{name:'Refresh internal providers'}));
    expect(api.internalAiStatus).toHaveBeenLastCalledWith(true);
  });
  it('says why a route cannot be used instead of showing a status code',()=>{
    expect(routeStatus('check_http_401')).toBe('the provider did not accept this key');
    expect(routeStatus('check_http_429')).toBe('over its quota or rate limit today');
    expect(routeStatus('check_http_402')).toBe('the account has no balance');
    expect(routeStatus('check_no_answer_in_time')).toBe('did not answer within 15 seconds');
    expect(routeStatus('check_image_not_read')).toContain('did not read the test picture');
    expect(routeStatus('http_503')).toContain('503');
    expect(routeStatus('something_new')).toBe('something new');
  });
  it('keeps connection errors visible',async()=>{
    vi.mocked(api.internalAiStatus).mockRejectedValue({message:'Configuration could not be loaded'});
    render(<InternalAiStatus/>);
    expect((await screen.findByRole('alert')).textContent).toContain('could not be loaded');
    expect(screen.queryByText(/photo routes available/)).toBeNull();
  });
});
