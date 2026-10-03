import { useState } from 'react';

export function SnapshotPanel({ names, disabled, onTake, onRestore }: {
  names: string[]; disabled: boolean; onTake: (name: string) => void; onRestore: (name: string) => void;
}): JSX.Element {
  const [name, setName] = useState('');
  const trimmed = name.trim();
  const duplicate = names.includes(trimmed);
  return <details className="lr-section"><summary>Named snapshots ({names.length})</summary>
    <div className="lr-section-body">
      <p className="lr-hint">Keep a version of this edit. Restoring a snapshot can be undone.</p>
      <label>Snapshot name<input value={name} maxLength={80} disabled={disabled} onChange={event => setName(event.target.value)}
        onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); if (!disabled && trimmed && !duplicate) onTake(trimmed); } }} /></label>
      {duplicate && <p role="status">Choose a different name; this snapshot already exists.</p>}
      <button type="button" disabled={disabled || !trimmed || duplicate} onClick={() => onTake(trimmed)}>Save snapshot</button>
      {names.map(saved => <div className="snapshot-row" key={saved}><span>{saved}</span><button type="button" disabled={disabled}
        aria-label={`Restore snapshot ${saved}`} onClick={() => onRestore(saved)}>Restore</button></div>)}
    </div>
  </details>;
}
