import { useEffect, useRef, useState, type ReactNode, type UIEvent } from 'react';
import type { ColorReference } from '../../ipc/colorMatch';
import './referenceComparison.css';

/** Both panes stay mounted so changing views retains the active photo and its preview. */
export function ReferenceComparison({ enabled, reference, children }: {
  enabled: boolean; reference: ColorReference | null; children: ReactNode;
}): JSX.Element {
  const left = useRef<HTMLDivElement>(null);
  const right = useRef<HTMLDivElement>(null);
  const [sync, setSync] = useState(true);
  const [referenceZoom, setReferenceZoom] = useState(1);
  const [activeZoom, setActiveZoom] = useState(1);
  const [failed, setFailed] = useState(false);
  useEffect(() => { setFailed(false); }, [reference?.id, reference?.preview, reference?.thumbnail]);
  const source = reference?.preview || reference?.thumbnail;
  const scroll = (event: UIEvent<HTMLDivElement>, other: HTMLDivElement | null) => {
    if (!sync || !other) return;
    const here = event.currentTarget;
    const x = here.scrollLeft / Math.max(1, here.scrollWidth - here.clientWidth);
    const y = here.scrollTop / Math.max(1, here.scrollHeight - here.clientHeight);
    const targetX = x * Math.max(0, other.scrollWidth - other.clientWidth);
    const targetY = y * Math.max(0, other.scrollHeight - other.clientHeight);
    if (Math.abs(other.scrollLeft - targetX) > .5) other.scrollLeft = targetX;
    if (Math.abs(other.scrollTop - targetY) > .5) other.scrollTop = targetY;
  };
  const zoomOptions = <><option value={1}>Fit</option><option value={1.5}>150%</option><option value={2}>200%</option><option value={3}>300%</option></>;
  return <div className={`reference-comparison${enabled ? ' is-comparing' : ''}`}>
    <div className="reference-comparison-controls" hidden={!enabled}>
      <strong>Reference / Active</strong>
      <label><input type="checkbox" checked={sync} onChange={event => {
        setSync(event.target.checked); if (event.target.checked) setReferenceZoom(activeZoom);
      }} />Sync zoom and pan</label>
      {!sync && <label>Reference zoom<select aria-label="Reference zoom" value={referenceZoom} onChange={event => setReferenceZoom(Number(event.target.value))}>{zoomOptions}</select></label>}
      <label>{sync ? 'View' : 'Active zoom'}<select aria-label={sync ? 'Comparison zoom' : 'Active zoom'} value={activeZoom} onChange={event => {
        const zoom = Number(event.target.value); setActiveZoom(zoom); if (sync) setReferenceZoom(zoom);
      }}>{zoomOptions}</select></label>
    </div>
    <div className="reference-comparison-grid">
      <figure className="reference-comparison-reference" hidden={!enabled} aria-label="Colour reference preview">
        <figcaption><span>Reference</span><strong>{reference?.name ?? 'Choose a reference'}</strong></figcaption>
        <div className="reference-comparison-pan" ref={left} onScroll={event => scroll(event, right.current)} tabIndex={enabled && source ? 0 : -1} aria-label="Pan reference photograph">
          <div className="reference-comparison-plane" style={{ width: `${referenceZoom * 100}%`, height: `${referenceZoom * 100}%` }}>
            {source && !failed ? <img src={source} alt={`Colour reference: ${reference?.name ?? ''}`} onError={() => setFailed(true)} />
              : <div className="reference-comparison-empty"><strong>{reference ? 'Reference preview unavailable' : 'Your reference goes here'}</strong><p>{reference ? 'This saved reference has colour measurements without a viewable photo. Upload the image again to compare it.' : 'Upload an edited photo or choose one from your reference library.'}</p></div>}
          </div>
        </div>
        {source && !reference?.preview && <small>Library thumbnail. Upload this reference again for a larger preview.</small>}
      </figure>
      <figure className="reference-comparison-active" aria-label={enabled ? 'Active photograph preview' : undefined}>
        <figcaption hidden={!enabled}><span>Active</span><strong>Your photograph</strong></figcaption>
        <div className="reference-comparison-pan" ref={right} onScroll={event => scroll(event, left.current)} tabIndex={enabled ? 0 : -1} aria-label={enabled ? 'Pan active photograph' : undefined}>
          <div className="reference-comparison-plane" style={enabled ? { width: `${activeZoom * 100}%`, height: `${activeZoom * 100}%` } : undefined}>{children}</div>
        </div>
      </figure>
    </div>
  </div>;
}
