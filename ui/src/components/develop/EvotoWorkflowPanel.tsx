import { useEffect, useRef, useState } from 'react';
import { asIpcError, pickPhotoFolder } from '../../ipc/client';
import {
  EXPORT_KINDS, FRAMING_PRESETS, evotoWorkflow, type BlinkReport, type ExportKind, type FramingPreset,
} from '../../ipc/evotoWorkflow';

export type EvotoWorkflowPanelProps = {
  projectId: string;
  photoId: string;
  disabled: boolean;
  /** Run a change to the photograph's recipe through the editor, which refreshes the preview. */
  onWrite: (action: () => Promise<unknown>) => Promise<unknown>;
  /** Open a photograph the blink check flagged. */
  onOpenPhoto?: (photoId: string) => void;
};

const describe = (name: string, closed: number, faces: number) =>
  `${name}: ${closed} of ${faces} face${faces === 1 ? '' : 's'} showing no white of the eye`;

/**
 * Framing, delivery and review in the style of Evoto (ADR-0150): ID photographs and measured
 * crops saved as ordinary undoable framing, cut-outs and social exports written only into a
 * folder the photographer chooses, and a blink check over the whole collection.
 */
export function EvotoWorkflowPanel({ projectId, photoId, disabled, onWrite, onOpenPhoto }: EvotoWorkflowPanelProps): JSX.Element {
  const [notes, setNotes] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [folder, setFolder] = useState('');
  const [rows, setRows] = useState(1);
  const [slides, setSlides] = useState(3);
  const [blinks, setBlinks] = useState<BlinkReport | null>(null);
  const shown = useRef(`${projectId}/${photoId}`);
  useEffect(() => { shown.current = `${projectId}/${photoId}`; setNotes([]); setError(null); }, [projectId, photoId]);
  const locked = disabled || busy;

  const run = async (task: () => Promise<string[]>) => {
    const asked = shown.current;
    setBusy(true); setError(null); setNotes([]);
    try {
      const said = await task();
      if (shown.current === asked) setNotes(said);
    } catch (cause) {
      if (shown.current === asked) setError(asIpcError(cause).message);
    } finally {
      setBusy(false);
    }
  };
  const frame = (preset: FramingPreset) => void run(async () => {
    // The editor reports a failed write itself; only a completed one is described here.
    const done: { reasons?: string[] } = {};
    await onWrite(async () => { done.reasons = (await evotoWorkflow.frame(projectId, photoId, preset)).reasons; });
    if (!done.reasons) return [];
    return done.reasons.length ? done.reasons : ['Framing saved. Undo takes it back.'];
  });
  const exportAs = (kind: ExportKind) => void run(async () => {
    if (!folder.trim()) throw new Error('Choose a folder first.');
    const count = kind === 'grid' ? rows : kind === 'carousel' ? slides : undefined;
    const made = await evotoWorkflow.export(projectId, photoId, kind, folder.trim(), count);
    return [`Wrote ${made.files.length} file${made.files.length === 1 ? '' : 's'} (${made.width} x ${made.height}).`, ...made.reasons, ...made.files];
  });
  const checkBlinks = () => void run(async () => {
    const report = await evotoWorkflow.blinkCheck(projectId);
    setBlinks(report);
    return [`Checked ${report.checked} photographs: ${report.flagged.length} to look at, ${report.withoutFaces} without a face.`];
  });

  return <div className="evoto-workflow" aria-label="Framing and delivery">
    <h4>Framing</h4>
    <p className="lr-hint">Measured from the main face and saved as an ordinary crop you can adjust or undo. ID photographs are levelled and put on white.</p>
    <div className="lr-crops" aria-label="Framing presets">{FRAMING_PRESETS.map(([preset, label, hint]) =>
      <button type="button" key={preset} title={hint} disabled={locked} onClick={() => frame(preset)}>{label}</button>)}
    </div>

    <h4>Export</h4>
    <div className="evoto-workflow-folder">
      <label>Folder<input type="text" aria-label="Export folder" value={folder} placeholder="Choose a folder" disabled={locked}
        onChange={event => setFolder(event.target.value)} /></label>
      <button type="button" disabled={locked} onClick={() => void pickPhotoFolder('Choose where to write the files').then(chosen => { if (chosen) setFolder(chosen); }).catch(() => undefined)}>Choose...</button>
    </div>
    <label>Grid rows<select aria-label="Grid rows" value={rows} disabled={locked} onChange={event => setRows(Number(event.target.value))}>
      {[1, 2, 3].map(n => <option key={n} value={n}>{n} ({n * 3} squares)</option>)}</select></label>
    <label>Carousel slides<input type="number" aria-label="Carousel slides" min={2} max={10} value={slides} disabled={locked}
      onChange={event => setSlides(Math.min(10, Math.max(2, Math.round(Number(event.target.value) || 2))))} /></label>
    <div className="lr-crops" aria-label="Exports">{EXPORT_KINDS.map(([kind, label, hint]) =>
      <button type="button" key={kind} title={hint} disabled={locked || !folder.trim()} onClick={() => exportAs(kind)}>{label}</button>)}
    </div>

    <h4>Review</h4>
    <button type="button" disabled={locked} onClick={checkBlinks}>Find faces showing no white of the eye</button>
    <p className="lr-hint">A list to look through, not a blink verdict: closed eyes land here, and so do squints, glare on
      glasses and faces under a veil, while a closed lid with pale shimmer can be missed. Telling them apart needs a
      trained eye-state model, which this build does not have.</p>
    {blinks && blinks.flagged.length > 0 && <ul aria-label="Faces showing no white of the eye">{blinks.flagged.map(item =>
      <li key={item.photoId}>{onOpenPhoto
        ? <button type="button" onClick={() => onOpenPhoto(item.photoId)}>{describe(item.fileName, item.closed, item.faces.length)}</button>
        : <span>{describe(item.fileName, item.closed, item.faces.length)}</span>}
      </li>)}</ul>}

    {busy && <p role="status">Working...</p>}
    {error && <p role="alert" className="lr-notice">{error}</p>}
    {notes.length > 0 && <ul className="lr-hint" aria-label="What happened">{notes.map(note => <li key={note}>{note}</li>)}</ul>}
  </div>;
}
