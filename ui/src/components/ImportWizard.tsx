import { useState } from 'react';

export type ImportWizardProps = {
  disabled: boolean;
  onStart: (roots: string[]) => void;
  onCancel: () => void;
  running: boolean;
  done: number;
  total: number;
};

/**
 * Folder picker plus progress. The picker takes typed paths so the component
 * stays testable without the native dialog; the shell replaces the input with a
 * real dialog when it is present.
 */
export function ImportWizard({
  disabled,
  onStart,
  onCancel,
  running,
  done,
  total,
}: ImportWizardProps): JSX.Element {
  const [draft, setDraft] = useState('');
  const [roots, setRoots] = useState<string[]>([]);

  const addRoot = (): void => {
    const trimmed = draft.trim();
    if (trimmed.length > 0 && !roots.includes(trimmed)) {
      setRoots([...roots, trimmed]);
    }
    setDraft('');
  };

  const percent = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;

  return (
    <section className="panel" aria-label="Import">
      <h2>Import</h2>

      <div className="row">
        <label htmlFor="root-input">Card or folder</label>
        <input
          id="root-input"
          value={draft}
          placeholder="D:\\DCIM\\100CANON"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              addRoot();
            }
          }}
        />
        <button type="button" onClick={addRoot}>
          Add folder
        </button>
      </div>

      {roots.length > 0 && (
        <ul className="root-list">
          {roots.map((root) => (
            <li key={root}>
              <code>{root}</code>
              <button
                type="button"
                aria-label={`Remove ${root}`}
                onClick={() => setRoots(roots.filter((entry) => entry !== root))}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <div className="row">
        <button
          type="button"
          disabled={disabled || running || roots.length === 0}
          onClick={() => onStart(roots)}
        >
          Start import
        </button>
        <button type="button" disabled={!running} onClick={onCancel}>
          Stop
        </button>
      </div>

      {running && (
        <div className="progress" role="progressbar" aria-valuenow={percent} aria-valuemin={0} aria-valuemax={100}>
          <div className="progress-bar" style={{ width: `${percent}%` }} />
          <span className="progress-label">
            {done} of {total || '?'} files
          </span>
        </div>
      )}
    </section>
  );
}
