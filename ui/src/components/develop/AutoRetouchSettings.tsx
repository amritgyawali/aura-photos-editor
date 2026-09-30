import { useState } from 'react';
import { DEFAULT_AUTO_RETOUCH, type AutoRetouchOptions } from '../../ipc/nativeRetouch';

const FEATURES: [keyof Omit<AutoRetouchOptions, 'intensity'>, string, string][] = [
  ['blemishes', 'Heal blemishes', 'Small spots redder than the surrounding skin. Moles and freckles are always kept.'],
  ['refine', 'Soften lines and redness', 'Fine lines, smile lines and redness beside the nose, only where they measure stronger than the cheek.'],
  ['eyes', 'Eyes', 'Iris detail, redness in the whites, flash red-eye and under-eye shadows, only where measured.'],
  ['teeth', 'Teeth', 'Reduce a measured yellow cast on visible teeth.'],
];

/** Choose how the automatic face and skin retouch works, then run it again. */
export function AutoRetouchSettings({ disabled, onRun }: { disabled: boolean; onRun: (options: AutoRetouchOptions) => void }) {
  const [options, setOptions] = useState<AutoRetouchOptions>(DEFAULT_AUTO_RETOUCH);
  const label = options.intensity < 0.8 ? 'Subtle' : options.intensity > 1.2 ? 'Polished' : 'Natural';
  return <details className="auto-retouch-settings">
    <summary>Automatic face and skin retouch settings</summary>
    <p className="lr-hint">Skin smoothing, tone and light always run. Everything is measured against the same person's own skin, and every result becomes an ordinary operation you can adjust or remove.</p>
    <label>Strength: {label} ({Math.round(options.intensity * 100)}%)
      <input type="range" min={25} max={150} step={5} value={Math.round(options.intensity * 100)} disabled={disabled}
        aria-label="Automatic retouch strength" onChange={event => setOptions({ ...options, intensity: Number(event.target.value) / 100 })} />
    </label>
    {FEATURES.map(([key, name, hint]) => <label key={key} className="retouch-toggle" title={hint}>
      <input type="checkbox" checked={options[key]} disabled={disabled} onChange={event => setOptions({ ...options, [key]: event.target.checked })} />{name}
    </label>)}
    <button type="button" disabled={disabled} onClick={() => onRun(options)}>Re-run automatic retouch</button>
    <p className="lr-hint">This replaces the automatic operations with new ones using these settings; operations you added yourself are kept. Undo restores the previous version.</p>
  </details>;
}
