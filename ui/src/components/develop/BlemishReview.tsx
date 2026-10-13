import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { nativeRetouch, type NativeRetouchEdit, type BlemishReview as Review } from '../../ipc/nativeRetouch';

/** Candidate areas are review aids, never an automatic claim that all acne is gone. */
export function BlemishReview({ projectId, photoId, recipeHash, selection, imageSrc, disabled, canPrepare, onPrepare, onChecking }: {
  projectId: string; photoId: string; recipeHash: string; selection: NativeRetouchEdit;
  imageSrc?: string | null; disabled: boolean; canPrepare: boolean;
  onPrepare: (region: NativeRetouchEdit['region']) => void;
  onChecking: (checking: boolean) => void;
}) {
  const [report, setReport] = useState<Review | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState('');
  const [offset, setOffset] = useState(0);
  const [quality, setQuality] = useState<'full'|'fast'>('full');
  const request = useRef(0);
  const busy = useRef(false);
  const mounted = useRef(true);
  const selectionKey = JSON.stringify(selection);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; request.current++; }; }, []);
  useEffect(() => { request.current++; setReport(null); setError(''); setOffset(0); }, [projectId,photoId,recipeHash,selectionKey,quality]);
  const inspect = async () => {
    if (busy.current || disabled || !recipeHash) return;
    busy.current = true;
    const ticket = ++request.current;
    setChecking(true); setReport(null); setError(''); onChecking(true);
    try {
      const next = await nativeRetouch.review(projectId,photoId,recipeHash,selection,quality);
      if (mounted.current && ticket === request.current && next.recipeHash === recipeHash) setReport(next);
    } catch (cause) { if (mounted.current && ticket === request.current) setError(asIpcError(cause).message); }
    finally { busy.current = false; if(mounted.current) setChecking(false); onChecking(false); }
  };
  return <details className="retouch-setting-group" open>
    <summary>Review remaining marks</summary>
    <p>Checks the saved photo inside your current selection. Choose detected face or body skin to check that region. Unsaved effects are excluded.</p>
    <label>Check resolution <select aria-label="Blemish check resolution" disabled={disabled || checking} value={quality} onChange={event=>setQuality(event.target.value as 'full'|'fast')}>
      <option value="full">Full resolution</option><option value="fast">Quick check</option>
    </select></label>
    <button type="button" disabled={disabled || checking || !recipeHash} onClick={()=>void inspect()}>
      {checking ? 'Checking remaining marks…' : 'Check remaining marks'}
    </button>
    {error && <p role="alert">{error}</p>}
    {report && <>
      <p role="status">{report.review.measuredAreas ? `${report.review.measuredAreas} possible mark area(s) measured.` : 'No candidate areas measured in this selection.'} Checked at {report.width} × {report.height}. This does not verify complete removal; inspect the full-resolution photo.</p>
      {imageSrc && report.review.regions.length > 0 && <div style={{position:'relative',maxWidth:280}}>
        <img src={imageSrc} alt="Saved photo with candidate mark locations" style={{width:'100%',display:'block'}}/>
        <svg aria-hidden="true" viewBox="0 0 1 1" preserveAspectRatio="none" style={{position:'absolute',inset:0,width:'100%',height:'100%',pointerEvents:'none'}}>
          {report.review.regions.map(({region},index)=><ellipse key={index} cx={region[0]} cy={region[1]} rx={region[2]} ry={region[3]} fill="none" stroke="#ffb545" strokeWidth="1.5" vectorEffect="non-scaling-stroke"/>)}
        </svg>
      </div>}
      {report.review.regions.slice(offset,offset+12).map(({region,pixels},index)=><p key={offset+index}>
        Area {offset+index+1} · {pixels} measured pixels · {Math.round(region[0]*100)}%, {Math.round(region[1]*100)}%{' '}
        <button type="button" disabled={disabled || !canPrepare} onClick={()=>onPrepare(region)}>Prepare blemish brush here</button>
      </p>)}
      {report.review.regions.length > 12 && <div>
        <button type="button" disabled={disabled || !offset} onClick={()=>setOffset(v=>Math.max(0,v-12))}>Previous areas</button>{' '}
        <button type="button" disabled={disabled || offset+12>=report.review.regions.length} onClick={()=>setOffset(v=>v+12)}>Next areas</button>
      </div>}
      {report.review.measuredAreas > report.review.regions.length && <p>Largest 256 areas listed across the selection. Repair and check again to review more.</p>}
      {!canPrepare && <p>Apply or discard your draft before preparing another brush.</p>}
      <p>Preparing a brush does not save an edit. Preview it, adjust it, then use Apply retouch to keep it.</p>
    </>}
  </details>;
}
