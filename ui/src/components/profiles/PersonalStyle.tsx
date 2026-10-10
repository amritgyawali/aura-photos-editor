import { useState } from 'react';
import { asIpcError } from '../../ipc/client';
import type { LearnedStyle } from './profileSelection';

type Props = {
  disabled: boolean;
  /** Ask the desktop for a `.lrcat`; null when the photographer cancels. */
  pick: () => Promise<string | null>;
  learn: (catalog: string, name: string) => Promise<LearnedStyle>;
  onLearned: (style: LearnedStyle) => void;
};

/**
 * "Teach AURA my style": read the develop settings of every photograph in a Lightroom Classic
 * catalogue and turn what the photographer consistently does into a profile of their own. The
 * catalogue is copied and read, never written, and no photograph is opened.
 */
export function PersonalStyle({ disabled, pick, learn, onLearned }: Props): JSX.Element {
  const [name, setName] = useState('My style');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<LearnedStyle | null>(null);

  const start = async (): Promise<void> => {
    setError(null);
    try {
      const catalog = await pick();
      if (!catalog) return;
      setBusy(true);
      const style = await learn(catalog, name);
      setResult(style);
      onLearned(style);
    } catch (cause) {
      setError(asIpcError(cause).message);
    } finally {
      setBusy(false);
    }
  };

  return <div className="personal-style" aria-busy={busy}>
    <div>
      <strong>Teach AURA your style</strong>
      <p>Choose your Lightroom Classic catalogue. AURA reads how you edited every photograph in it and builds a profile of your own: it still measures each photo's exposure and white balance, then adds the contrast, tones, colour and finishing you consistently use.</p>
    </div>
    <label>Style name<input type="text" value={name} maxLength={60} disabled={disabled || busy} onChange={event => setName(event.target.value)} /></label>
    <button type="button" disabled={disabled || busy} onClick={() => void start()}>{busy ? 'Learning…' : 'Learn from Lightroom…'}</button>
    {error && <p role="alert" className="reference-error">{error}</p>}
    {result && <div className="personal-style-result" role="status">
      <p>Learned “{result.profile.name}” from {result.photos} of {result.edited} edited photographs.</p>
      {result.findings.length > 0 && <ul>{result.findings.slice(0, 12).map(finding => <li key={finding}>{finding}</li>)}</ul>}
    </div>}
  </div>;
}
