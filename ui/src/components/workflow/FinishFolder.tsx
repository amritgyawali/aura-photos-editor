import { useState } from 'react';

import { asIpcError, automaticStart, inTauri, pickPhotoFolder, type ProfileSelection } from '../../ipc/client';
import type { AutomaticLookInput } from '../../ipc/types';
import { useAutomatic } from '../../state/automaticStore';
import type { ReferenceSelection } from '../look/referenceStyle';

const CULL_KEY = 'aura.finish.cull';
const EXPORT_KEY = 'aura.finish.export';

function saved(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}
function remember(key: string, value: string): void {
  try { if (value) localStorage.setItem(key, value); else localStorage.removeItem(key); } catch { /* The choice still applies to this run. */ }
}

/** The look chosen in steps one and two, as whole percentages. ADR-0088. */
export function lookFor(profile: ProfileSelection | null, reference: ReferenceSelection | null): AutomaticLookInput | null {
  if (!profile && !reference) return null;
  return {
    profileId: profile?.profileId ?? null,
    profileStrength: Math.round(Math.max(0, Math.min(1.5, profile?.strength ?? 1)) * 100),
    referenceId: reference?.analysis.id ?? null,
    referenceStrength: Math.round(Math.max(0, Math.min(1, reference?.strength ?? 0)) * 100),
  };
}

/**
 * One press that finishes a folder: import, cull, edit, retouch and export, in that order,
 * on the native worker. Choosing the folder is the only thing a photographer has to do; the
 * run keeps going when they change tabs, and the panel above reports every phase and every
 * decision it could not make. Where the files go can be changed, because a wedding does not
 * always fit on the drive Pictures lives on.
 */
export function FinishFolder({ disabled, profile, reference, onStarted, onError }: {
  disabled: boolean; profile: ProfileSelection | null; reference: ReferenceSelection | null;
  onStarted: (projectId: string) => void; onError: (error: { code: string; message: string }) => void;
}): JSX.Element {
  const [cull, setCull] = useState(() => saved(CULL_KEY) !== 'off');
  const [folder, setFolder] = useState('');
  const [exportTo, setExportTo] = useState(() => saved(EXPORT_KEY) ?? '');
  const [choosing, setChoosing] = useState(false);
  const locked = disabled || choosing;
  const fail = (cause: unknown) => { const error = asIpcError(cause); onError({ code: error.code, message: error.message }); };
  const start = async () => {
    if (choosing) return;
    setChoosing(true);
    try {
      const root = folder.trim() || await pickPhotoFolder('Choose the folder to finish');
      if (!root) return;
      useAutomatic.setState({ starting: true, error: null });
      const run = await automaticStart({ roots: [root], projectId: null, look: lookFor(profile, reference),
        keepEverything: !cull, destination: exportTo.trim() || null });
      useAutomatic.getState().adopt(run.jobId, run.projectId);
      setFolder('');
      onStarted(run.projectId);
    } catch (cause) {
      useAutomatic.setState({ starting: false });
      fail(cause);
    } finally { setChoosing(false); }
  };
  const browseExport = async () => {
    try {
      const chosen = await pickPhotoFolder('Choose where the finished photos go');
      if (chosen) { setExportTo(chosen); remember(EXPORT_KEY, chosen); }
    } catch (cause) { fail(cause); }
  };
  return <div className="finish-folder">
    <button className="is-primary" type="button" disabled={!inTauri() || locked} onClick={() => void start()}>
      {choosing ? 'Starting…' : 'Finish a whole folder'}
    </button>
    <label className="finish-cull"><input type="checkbox" checked={cull} disabled={locked}
      onChange={event => { setCull(event.currentTarget.checked); remember(CULL_KEY, event.currentTarget.checked ? 'on' : 'off'); }} />
      <span>Cull first: leave out blurred, unusable and duplicate burst frames. Nothing is deleted.</span></label>
    <p className="studio-footnote">Import, cull, edit, retouch and export, with no other click. Every photo gets its own measured settings.</p>
    <details className="finish-options">
      <summary>Folder and export location</summary>
      <label>Folder to finish<input type="text" value={folder} disabled={locked} placeholder="Leave empty to choose with the button, or paste a path"
        onChange={event => setFolder(event.currentTarget.value)} /></label>
      <label>Export into (a new folder is made for each run)<span className="finish-export"><input type="text" value={exportTo} disabled={locked} placeholder="Pictures › AURA Exports"
        onChange={event => { setExportTo(event.currentTarget.value); remember(EXPORT_KEY, event.currentTarget.value.trim()); }} />
        <button type="button" disabled={!inTauri() || locked} onClick={() => void browseExport()}>Browse…</button></span></label>
    </details>
  </div>;
}
