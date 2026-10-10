import { api, asIpcError } from '../../ipc/client';
import { nativeRetouch, type AutoRetouchOptions } from '../../ipc/nativeRetouch';

export type RetouchOutcome = { photoId: string; name: string; outcome: 'retouched'|'skipped'|'failed'; detail: string };

/** Sync the requested style, measuring fresh skin/masks for each photograph. Never copy
 * coordinates, donors or masks between photos. Each native operation preserves manual edits. */
export async function retouchCollection(projectId: string, options: AutoRetouchOptions,
  stopped: () => boolean, progress: (message: string) => void,
  completed: (result: RetouchOutcome) => void): Promise<RetouchOutcome[]> {
  const photos = [];
  const seen = new Set<string>();
  for (let offset=0; !stopped(); offset+=240) {
    const page=await api.listImages({projectId,offset,limit:240,orderBy:'timeline'});
    for (const photo of page) if (!seen.has(photo.id)) { seen.add(photo.id); photos.push(photo); }
    if (page.length<240) break;
  }
  const outcomes: RetouchOutcome[]=[];
  for (const [index, photo] of photos.entries()) {
    if (stopped()) break;
    progress(`Retouching ${index+1} of ${photos.length}: ${photo.fileName}`);
    let result: RetouchOutcome;
    try {
      const recipe=await nativeRetouch.autoRetouch(projectId,photo.id,options);
      const body=JSON.parse(recipe.body);
      const report=body.studio_portrait_auto_v1;
      if (!report || typeof report.operations!=='number') throw new Error('Retouch returned no saved outcome.');
      result={photoId:photo.id,name:photo.fileName,outcome:report.operations>0?'retouched':'skipped',
        detail:typeof report.message==='string'?report.message:'Cleanup saved.'};
    } catch(error) {
      result={photoId:photo.id,name:photo.fileName,outcome:'failed',detail:asIpcError(error).message};
    }
    outcomes.push(result); completed(result);
  }
  progress(`${stopped()?'Stopped after the current photo. ':''}${outcomes.filter(r=>r.outcome==='retouched').length} retouched, ${outcomes.filter(r=>r.outcome==='skipped').length} skipped, ${outcomes.filter(r=>r.outcome==='failed').length} failed. ${Math.max(0,photos.length-outcomes.length)} remaining.`);
  return outcomes;
}
