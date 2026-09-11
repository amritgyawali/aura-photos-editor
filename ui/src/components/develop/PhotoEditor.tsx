import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { api, asIpcError, develop, photoAutoEdit } from '../../ipc/client';
import type { HistoryDto, PhotoAutoEditDto, RecipeDto, RenderDto } from '../../ipc/types';
import { rgbDataUrl } from './rgbImage';

const controls = [
  ['global.exposure', 'Exposure (stops)', -5, 5, 0.1],
  ['global.contrast', 'Contrast', -100, 100, 1],
  ['global.highlights', 'Highlights', -100, 100, 1],
  ['global.shadows', 'Shadows', -100, 100, 1],
  ['global.vibrance', 'Vibrance', -100, 100, 1],
] as const;

/** The everyday workflow stays usable even when advanced analysis has not run. */
export function PhotoEditor({ projectId, photoId, onChanged, render, recipe, history }: {
  projectId: string; photoId: string; onChanged: () => Promise<void>;
  render: RenderDto | null; recipe: RecipeDto | null; history: HistoryDto | null;
}): JSX.Element {
  const [original, setOriginal] = useState<string | null>(null);
  const [answer, setAnswer] = useState<PhotoAutoEditDto | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [before, setBefore] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);
  const stopped = useRef(false);
  const activeJob = useRef<string | null>(null);
  const editedSrc = useMemo(() => render ? rgbDataUrl(render) : null, [render]);

  const reload = useCallback(async () => {
    const preview = await api.getPreview({ projectId, photoId, level: 'proxy', priority: 'interactive' });
    setOriginal(preview.dataUrl);
  }, [photoId, projectId]);

  useEffect(() => {
    void reload().catch((e: unknown) => setError(asIpcError(e).message));
    return () => {
      stopped.current = true;
      if (activeJob.current) void api.cancelJob(activeJob.current);
    };
  }, [reload]);

  const write = async (action: () => Promise<unknown>): Promise<void> => {
    setBusy(true); setError(null);
    try { await action(); await onChanged(); }
    catch (e) { setError(asIpcError(e).message); }
    finally { setBusy(false); }
  };

  const autoEdit = async (wholeProject: boolean): Promise<void> => {
    stopped.current = false;
    await write(async () => {
      const ids: string[] = [];
      if (wholeProject) {
        for (let offset = 0; ; offset += 250) {
          const page = await api.listImages({ projectId, offset, limit: 250, orderBy: 'timeline' });
          ids.push(...page.map((row) => row.id));
          if (page.length < 250 || stopped.current) break;
        }
      } else ids.push(photoId);
      let completed = 0;
      let local = 0;
      const failures: string[] = [];
      for (const id of ids) {
        if (stopped.current) break;
        const jobId = crypto.randomUUID();
        activeJob.current = jobId;
        setProgress(`Editing ${completed + failures.length + 1} of ${ids.length}…`);
        try {
          const result = await photoAutoEdit({ projectId, photoId: id, jobId });
          completed++;
          if (result.source === 'local_fallback') local++;
          if (id === photoId) setAnswer(result);
        } catch (e) {
          if (!stopped.current) failures.push(asIpcError(e).message);
        } finally { activeJob.current = null; }
      }
      setProgress(`${stopped.current ? 'Stopped. ' : ''}${completed} photo${completed === 1 ? '' : 's'} edited. ${local} used local enhancement.${failures.length ? ` ${failures.length} failed.` : ''}`);
      if (failures.length) setError(failures[0] ?? 'Some photographs could not be edited.');
    });
  };

  return <section className="photo-editor" aria-label="Photo editor">
    <div className="row">
      <button type="button" disabled={busy || !recipe} onClick={() => void autoEdit(false)}>Auto edit photo</button>
      <button type="button" disabled={busy || !recipe} onClick={() => void autoEdit(true)}>Auto edit all photos</button>
      {busy && <button type="button" onClick={() => {
        stopped.current = true;
        if (activeJob.current) void api.cancelJob(activeJob.current).catch((e: unknown) => setError(asIpcError(e).message));
      }}>Stop editing</button>}
      <button type="button" disabled={!original} aria-pressed={before} onClick={() => setBefore(!before)}>{before ? 'Show edited' : 'Show original'}</button>
    </div>
    <p>Automatic edits preserve your manual settings. Connect a vision model in AI settings for photo-aware editing; otherwise AURA uses local brightness enhancement.</p>
    {error && <p role="alert">{error}</p>}
    {progress && <p role="status">{progress}</p>}
    <div className="photo-editor__viewer">
      {(before ? original : editedSrc) ? <img src={(before ? original : editedSrc) ?? ''} alt={before ? 'Original photograph' : 'Edited photograph'} /> : <p>{error ? 'Preview unavailable.' : 'Loading photograph…'}</p>}
      <span>{before ? 'Original' : 'Edited'}</span>
    </div>
    {answer && <div role="status"><strong>{answer.source === 'local_fallback' ? 'Local enhancement' : `AI edit · ${answer.model}`}</strong><p>{answer.reasons.join(' ')}</p></div>}
    <fieldset disabled={busy || !recipe} className="photo-editor__controls">
      <legend>Fine-tune your edit</legend>
      {controls.map(([path, label, min, max, step]) => {
        const param = recipe?.params.find((p) => p.path === path);
        return <label key={`${photoId}:${path}:${String(param?.value)}`}>{label}
          <input type="number" defaultValue={Number(param?.value ?? 0)} min={min} max={max} step={step}
            onBlur={(event) => {
              const value = event.target.valueAsNumber;
              if (event.target.value === '' || !Number.isFinite(value) || value < min || value > max) {
                event.target.value = String(param?.value ?? 0); return;
              }
              if (value !== param?.value) void write(() => develop.setParam({ projectId, photoId, path, value, label }));
            }} />
          {param?.protected && <small>You set this</small>}
        </label>;
      })}
      <button type="button" disabled={!history?.canUndo} onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'undo' }))}>Undo edit</button>
      <button type="button" onClick={() => void write(() => develop.historyStep({ projectId, photoId, action: 'reset_original' }))}>Reset photo</button>
    </fieldset>
  </section>;
}
