import { useEffect, useRef, useState } from 'react';
import { asIpcError } from '../../ipc/client';
import { photoAnalysis, type PhotoAnalysisReport, type PhotoDiagnostic } from '../../ipc/photoAnalysis';
import type { RecipeDto } from '../../ipc/types';
import { QUALITY_GROUPS, type QualityGroup } from '../../ipc/qualityTools';

export type PhotoAnalysisPanelProps = {
  projectId: string;
  photoId: string;
  recipe: RecipeDto | null;
  disabled: boolean;
  focusGroup?: QualityGroup;
};
const GROUPS: ReadonlyArray<readonly [string, ReadonlySet<string>]> = [
  ['Scopes', new Set(['rgb-parade', 'luma-waveform', 'chroma-vectorscope'])],
  ['Exposure & colour', new Set(['exposure-map', 'range-map', 'neutral-cast', 'dynamic-range', 'exposure-zones', 'srgb-occupancy', 'illumination-map', 'channel-correlation', 'highlight-chroma'])],
  ['Detail & artifacts', new Set(['focus-map', 'noise-map', 'local-contrast', 'stripe-profile', 'block-period', 'edge-directions', 'edge-registration', 'detail-scales'])],
  ...QUALITY_GROUPS.map(([label, ids]) => [label, new Set<string>(ids)] as const),
];
function number(value: number): string {
  return value.toLocaleString(undefined, { maximumFractionDigits: Math.abs(value) < 1 ? 5 : 2 });
}
function Visual({ tool }: { tool: PhotoDiagnostic }): JSX.Element {
  if (tool.kind === 'bars') {
    const scale = Math.max(Math.abs(tool.low), Math.abs(tool.high), 1e-6);
    const hasNegative = tool.low < 0;
    return <svg role="img" aria-label={`${tool.title} chart`} viewBox="0 0 320 112" style={{ width: '100%', maxHeight: 160 }}>
      <line x1="0" x2="320" y1={hasNegative ? 54 : 104} y2={hasNegative ? 54 : 104} stroke="currentColor" opacity="0.3" />
      {tool.values.map((value, index) => {
        const height = Math.abs(value) / scale * (hasNegative ? 48 : 96);
        const step = 320 / tool.values.length;
        return <rect key={index} x={index * step + 1} y={hasNegative ? (value >= 0 ? 54 - height : 54) : 104 - height}
          width={Math.max(1, step - 2)} height={height} fill={value < 0 ? '#c49cff' : '#73d3ca'}>
          <title>{tool.labels[index] ?? `${index + 1}`}: {number(value)} {tool.unit}</title>
        </rect>;
      })}
    </svg>;
  }
  const density = tool.kind === 'density';
  const low = density ? 0 : Math.min(tool.low, 0);
  const span = Math.max(tool.high - low, 1e-6);
  return <svg role="img" aria-label={`${tool.title} chart`} viewBox={`0 0 ${tool.width} ${tool.height}`}
    preserveAspectRatio="none" style={{ width: '100%', height: density ? 150 : 120, background: '#131b24' }}>
    {tool.values.map((value, index) => {
      const position = density ? Math.log1p(Math.max(0, value)) / Math.log1p(Math.max(1, tool.high)) : (value - low) / span;
      const fill = density ? `rgba(110,222,204,${position})` : `hsl(${240 - position * 240} 75% ${25 + position * 35}%)`;
      return <rect key={index} x={index % tool.width} y={Math.floor(index / tool.width)} width="1" height="1" fill={fill}>
        <title>{number(value)} {tool.unit}</title>
      </rect>;
    })}
  </svg>;
}
function Tool({ tool }: { tool: PhotoDiagnostic }): JSX.Element {
  return <article style={{ padding: '0.8rem 0', borderBottom: '1px solid #ffffff18' }}>
    <h4 style={{ margin: '0 0 0.4rem' }}>{tool.title}</h4>
    <p style={{ fontSize: '0.8rem', opacity: 0.8 }}>{tool.description}</p>
    {tool.samples === 0 ? <p>No usable samples for this measurement.</p> : <>
      <Visual tool={tool} />
      <p style={{ fontSize: '0.75rem' }}>{number(tool.low)} to {number(tool.high)} {tool.unit}</p>
      {tool.labels.length > 0 && tool.values.length <= 15 && <dl style={{ display: 'grid', gridTemplateColumns: '1fr auto', gap: '0.2rem', fontSize: '0.8rem' }}>
        {tool.labels.map((label, index) => <div key={label} style={{ display: 'contents' }}><dt>{label}</dt><dd style={{ margin: 0 }}>{number(tool.values[index] ?? 0)} {tool.unit}</dd></div>)}
      </dl>}
    </>}
  </article>;
}

/** Explicit inspection, revision invalidation, and late-result protection. Never writes edits. */
export function PhotoAnalysisPanel({ projectId, photoId, recipe, disabled, focusGroup }: PhotoAnalysisPanelProps): JSX.Element {
  const [report, setReport] = useState<PhotoAnalysisReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [group, setGroup] = useState('Scopes');
  const [selected, setSelected] = useState('rgb-parade');
  const identity = `${projectId}:${photoId}:${recipe?.recipeHash ?? ''}`;
  const current = useRef(identity); current.current = identity;
  const request = useRef(0);
  useEffect(() => { request.current += 1; setReport(null); setBusy(false); setError(null); }, [identity]);
  useEffect(() => () => { current.current = ''; request.current += 1; }, []);
  useEffect(() => { if (focusGroup) setGroup(focusGroup); }, [focusGroup]);
  const run = async () => {
    if (!recipe || disabled || busy) return;
    const started = identity; const sequence = ++request.current;
    setBusy(true); setError(null);
    try {
      const result = await photoAnalysis.inspect(projectId, photoId, recipe.recipeHash);
      if (current.current !== started || request.current !== sequence) return;
      if (result.recipeHash !== recipe.recipeHash) { setError('Photo changed. Run the inspection again.'); return; }
      setReport(result);
    } catch (failure) {
      if (current.current === started && request.current === sequence) setError(asIpcError(failure).message);
    } finally {
      if (current.current === started && request.current === sequence) setBusy(false);
    }
  };
  const members = GROUPS.find(([label]) => label === group)?.[1];
  const tools = report?.diagnostics.filter(tool => members?.has(tool.id)) ?? [];
  const active = tools.find(tool => tool.id === selected) ?? tools[0];
  return <section aria-label="Photo inspection">
    <h3>Photo inspection</h3>
    <p>Explore 95 scopes and measurements of your saved edit across colour, texture, lighting, and delivery. Fine detail readings use a small preview; inspect the original at 100% before delivery.</p>
    <button disabled={disabled || busy || !recipe} onClick={() => { void run(); }}>{busy ? 'Inspecting…' : 'Run photo inspection'}</button>
    {error && <p role="alert">{error}</p>}
    {report && <>
      <p role="status">{report.diagnostics.length} measurements · {report.width} × {report.height} preview</p>
      {report.notes.length > 0 && <details><summary>Rendering notes</summary>{report.notes.map((note, index) => <p key={index}>{note}</p>)}</details>}
      <label>Inspection group<select aria-label="Inspection group" value={group} onChange={event => setGroup(event.target.value)}>
        {GROUPS.map(([name]) => <option key={name}>{name}</option>)}
      </select></label>
      <label>Measurement<select aria-label="Measurement" value={active?.id ?? ''} onChange={event => setSelected(event.target.value)}>
        {tools.map(tool => <option key={tool.id} value={tool.id}>{tool.title}</option>)}
      </select></label>
      {active && <Tool tool={active} />}
    </>}
  </section>;
}
