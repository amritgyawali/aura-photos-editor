import { useState } from 'react';
import { DEFAULT_AUTO_RETOUCH, type AutoRetouchOptions, type RetouchScope } from '../../ipc/nativeRetouch';

const SCOPES: [RetouchScope, string, string][] = [
  ['face', 'Face', 'Skin, blemishes, lines, eyes and teeth on each detected face.'],
  ['body', 'Body skin', 'Neck, shoulders, chest and arms below each face. The face is left as it is.'],
  ['face_and_body', 'Face + body skin', 'Everything on the face, plus matching body skin.'],
];

const FEATURES: [keyof Omit<AutoRetouchOptions, 'intensity' | 'scope'>, string, string][] = [
  ['blemishes', 'Heal blemishes', 'Small spots redder than the surrounding skin. Moles and freckles are always kept.'],
  ['refine', 'Soften lines and redness', 'Fine lines, smile lines and redness beside the nose, only where they measure stronger than the cheek.'],
  ['eyes', 'Eyes', 'Iris detail, redness in the whites, flash red-eye and under-eye shadows, only where measured.'],
  ['teeth', 'Teeth', 'Reduce a measured yellow cast on visible teeth.'],
];

/** Choose what the automatic retouch works on and how strongly, then run it. */
export function AutoRetouchSettings({ disabled, busy = false, onRun }: { disabled: boolean; busy?: boolean; onRun: (options: AutoRetouchOptions) => void }) {
  const [options, setOptions] = useState<AutoRetouchOptions>(DEFAULT_AUTO_RETOUCH);
  const label = options.intensity < 0.8 ? 'Subtle' : options.intensity > 1.2 ? 'Polished' : 'Natural';
  const faceFeatures = options.scope !== 'body';
  const scope = SCOPES.find(([value]) => value === options.scope) ?? SCOPES[0];
  return <fieldset className="auto-retouch-settings" disabled={disabled}>
    <legend>Automatic retouch</legend>
    <div role="radiogroup" aria-label="What to retouch" className="retouch-scope">
      {SCOPES.map(([value, name, hint]) => <label key={value} className="retouch-toggle" title={hint}>
        <input type="radio" name="auto-retouch-scope" value={value} checked={options.scope === value}
          onChange={() => setOptions({ ...options, scope: value })} />{name}
      </label>)}
    </div>
    <p className="lr-hint">{scope?.[2]} Skin is measured against the same person's own skin; every result becomes an ordinary operation you can adjust, disable or remove below.</p>
    <button type="button" className="retouch-primary" onClick={() => onRun(options)}>{busy ? 'Detecting and retouching…' : `Auto retouch: ${scope?.[1] ?? 'Face'}`}</button>
    <details>
      <summary>Strength and details</summary>
      <label>Strength: {label} ({Math.round(options.intensity * 100)}%)
        <input type="range" min={25} max={150} step={5} value={Math.round(options.intensity * 100)}
          aria-label="Automatic retouch strength" onChange={event => setOptions({ ...options, intensity: Number(event.target.value) / 100 })} />
      </label>
      {FEATURES.map(([key, name, hint]) => <label key={key} className="retouch-toggle" title={hint}>
        <input type="checkbox" checked={options[key]} disabled={!faceFeatures} onChange={event => setOptions({ ...options, [key]: event.target.checked })} />{name}
      </label>)}
      {!faceFeatures && <p className="lr-hint">Face details are not used while only body skin is selected.</p>}
    </details>
    <p className="lr-hint">Running it again replaces the automatic operations; operations you added yourself are kept. Undo restores the previous version.</p>
  </fieldset>;
}
