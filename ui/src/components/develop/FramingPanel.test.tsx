import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { FramingPanel, readFramingPresets } from './FramingPanel';
import { framingWorkflows } from '../../ipc/framingWorkflows';
import { delivery, pickPhotoFolder } from '../../ipc/client';
import type { RecipeDto } from '../../ipc/types';
const input = { photoId:'p', recipeHash:'h', body:JSON.stringify({ geometry:{ crop:[0,0,1,1],rotate:0,perspective:null } }) } as RecipeDto;
const measured = { width:6000,height:4000,recipeHash:'h',faces:[{ bounds:[.4,.2,.6,.4],eyes:[[.44,.28],[.56,.3]],eyesMeasured:2,confidence:.9 }],subjectBounds:[.2,.1,.8,.9],subjectAxis:90,reasons:[] };
vi.mock('../../ipc/framingWorkflows', () => ({ framingWorkflows:{context:vi.fn()} }));
vi.mock('../../ipc/client', () => ({ asIpcError:(e:Error)=>({message:e.message}), delivery:{exportRun:vi.fn()},pickPhotoFolder:vi.fn() }));
beforeEach(() => { vi.resetAllMocks(); localStorage.clear(); vi.mocked(framingWorkflows.context).mockResolvedValue(measured as never); });
function mount(recipe=input) { const save=vi.fn().mockResolvedValue(undefined); const result=render(<FramingPanel projectId="project" photoId="p" recipe={recipe} original="data:image/png;base64,x" disabled={false} onSave={save}/>); fireEvent.click(screen.getByText('Crop & framing studio')); return {save,...result}; }
it('prepares exact pixels without saving, then saves all geometry once', async () => {
  const {save}=mount(); fireEvent.click(screen.getByText('Measure source and subjects')); await screen.findByText('Original: 6000 × 4000 pixels.');
  fireEvent.click(screen.getByText('Prepare exact pixel crop')); expect(save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText('Apply framing')); await waitFor(()=>expect(save).toHaveBeenCalledTimes(1));
  const crop=save.mock.calls[0]?.[0].crop; expect((crop[2]-crop[0])*6000).toBeCloseTo(1200); expect((crop[3]-crop[1])*4000).toBeCloseTo(1500);
});
it('persists a framing template and refuses corrupted local data explicitly', () => {
  mount(); fireEvent.click(screen.getByText('Reusable framing templates'));
  fireEvent.change(screen.getByLabelText('Framing template name'),{target:{value:'Portrait print'}}); fireEvent.click(screen.getByText('Save framing template'));
  expect(readFramingPresets()[0]?.name).toBe('Portrait print');
  localStorage.setItem('aura-framing-presets-v1','[{"name":"Bad","ratio":0}]'); expect(()=>readFramingPresets()).toThrow(/invalid/);
  fireEvent.click(screen.getByText('Clear local framing templates')); expect(readFramingPresets()).toEqual([]);
});
it('discards a delayed measurement after the saved recipe changes', async () => {
  let finish:(value:unknown)=>void=()=>undefined; vi.mocked(framingWorkflows.context).mockReturnValue(new Promise(resolve=>{finish=resolve as never;}));
  const {rerender}=mount(); fireEvent.click(screen.getByText('Measure source and subjects'));
  rerender(<FramingPanel projectId="project" photoId="p" recipe={{...input,recipeHash:'new'}} original="x" disabled={false} onSave={vi.fn()}/>);
  await act(async()=>{finish(measured);}); expect(screen.queryByText('Original: 6000 × 4000 pixels.')).toBeNull();
});
it('does not announce a successful print when native verification failed', async () => {
  mount(); fireEvent.click(screen.getByText('Physical print sizing')); vi.mocked(pickPhotoFolder).mockResolvedValue('C:/export');
  vi.mocked(delivery.exportRun).mockResolvedValue({requested:1,written:0,verified:0,renderFailed:1,corrupt:0} as never);
  fireEvent.click(screen.getByText('Export this saved framed print'));
  await screen.findByRole('alert'); expect(screen.getByRole('alert').textContent).toMatch(/not completely written/); expect(screen.queryByText(/Framed print exported/)).toBeNull();
});
